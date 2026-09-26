//! Drives the server with LSP messages (no stdio).

use lsp_server::{Notification, Request, RequestId};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use stylet_lsp::{Server, Settings};
use stylet_resolve::ResolveConfig;

fn uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn open(server: &mut Server, path: &Path, text: &str) {
    let params = json!({ "textDocument": { "uri": uri(path), "languageId": "stylet", "version": 1, "text": text } });
    server.notification(Notification::new("textDocument/didOpen".into(), params));
}

fn request(server: &mut Server, method: &str, params: Value) -> Value {
    let response = server.request(Request::new(RequestId::from(1), method.into(), params));
    match response.response_result {
        Ok(value) => value,
        Err(e) => panic!("{method}: {}", e.message),
    }
}

/// `publishDiagnostics` per file name: messages.
fn diagnostics(server: &mut Server) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<_> = server
        .diagnostics()
        .into_iter()
        .map(|n| {
            let file = n.params["uri"]
                .as_str()
                .unwrap()
                .rsplit('/')
                .next()
                .unwrap()
                .to_string();
            let messages = n.params["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| {
                    format!(
                        "{}: {}",
                        d["range"]["start"]["line"],
                        d["message"].as_str().unwrap()
                    )
                })
                .collect();
            (file, messages)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn project() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(
        root.join("index.styl"),
        "@import 'placeholders'\n@import 'button'\n",
    )
    .unwrap();
    fs::write(
        root.join("placeholders.styl"),
        "$btn {\n  padding: 4px\n}\n",
    )
    .unwrap();
    fs::write(root.join("button.styl"), ".button {\n  @extend $btn\n}\n").unwrap();
    let mut server = Server::new(Settings {
        resolve: ResolveConfig {
            root: root.clone(),
            aliases: Vec::new(),
        },
        entries: vec![root.join("index.styl")],
        ..Settings::default()
    });

    // The placeholder is known through the entry, although button.styl doesn't import it.
    open(
        &mut server,
        &root.join("button.styl"),
        ".button {\n  @extend $btn\n  color: red;\n}\n",
    );
    assert_eq!(
        diagnostics(&mut server),
        [(
            "button.styl".into(),
            vec!["2: stylet doesn't use semicolons; end the line instead".to_string()]
        )]
    );

    // Unsaved changes count; fixed errors are cleared.
    open(
        &mut server,
        &root.join("button.styl"),
        ".button {\n  @extend $nope\n}\n",
    );
    assert_eq!(
        diagnostics(&mut server),
        [(
            "button.styl".into(),
            vec!["1: unknown placeholder `$nope`".to_string()]
        )]
    );

    // An open file outside every entry: syntax errors only.
    open(
        &mut server,
        &root.join("loose.styl"),
        ".a {\n  @extend $missing\n  color\n}\n",
    );
    let loose = diagnostics(&mut server)
        .into_iter()
        .find(|(f, _)| f == "loose.styl")
        .unwrap();
    assert_eq!(loose.1.len(), 1);

    // Formatting.
    open(
        &mut server,
        &root.join("button.styl"),
        ".button{color:red}\n",
    );
    let edits = request(
        &mut server,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri(&root.join("button.styl")) },
            "options": { "tabSize": 2, "insertSpaces": true }
        }),
    );
    assert_eq!(edits[0]["newText"], ".button {\n  color: red\n}\n");

    // Definitions: import path and placeholder.
    open(
        &mut server,
        &root.join("button.styl"),
        ".button {\n  @extend $btn\n}\n",
    );
    diagnostics(&mut server);
    let at = |path: &Path, line: u32, character: u32| json!({ "textDocument": { "uri": uri(path) }, "position": { "line": line, "character": character } });
    let import = request(
        &mut server,
        "textDocument/definition",
        at(&root.join("index.styl"), 1, 10),
    );
    assert!(
        import["uri"].as_str().unwrap().ends_with("/button.styl"),
        "{import}"
    );
    let placeholder = request(
        &mut server,
        "textDocument/definition",
        at(&root.join("button.styl"), 1, 12),
    );
    assert!(
        placeholder[0]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/placeholders.styl"),
        "{placeholder}"
    );

    // References include the definition when asked.
    let mut params = at(&root.join("placeholders.styl"), 0, 2);
    params["context"] = json!({ "includeDeclaration": true });
    let refs = request(&mut server, "textDocument/references", params);
    assert_eq!(refs.as_array().unwrap().len(), 2, "{refs}");

    // Document symbols.
    let symbols = request(
        &mut server,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri(&root.join("placeholders.styl")) } }),
    );
    assert_eq!(symbols[0]["name"], "$btn");
}

/// Labels of the completions at `|`, after a compile has found the project's files.
fn complete(server: &mut Server, path: &Path, text: &str) -> Vec<String> {
    let at = text.find('|').unwrap();
    let text = text.replace('|', "");
    open(server, path, &text);
    server.diagnostics();
    let line = text[..at].matches('\n').count();
    let character = at - text[..at].rfind('\n').map_or(0, |i| i + 1);
    let params = json!({
        "textDocument": { "uri": uri(path) },
        "position": { "line": line, "character": character },
    });
    let result = request(server, "textDocument/completion", params);
    result
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|i| i["label"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn completions() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("index.styl"), "@import 'theme'\n@import 'app'\n").unwrap();
    fs::write(
        root.join("theme.styl"),
        ":root {\n  --primary: #06c\n  --gap: 1rem\n}\n\n@custom-media --phone (width <= 600px)\n\n$panel {\n  padding: var(--gap)\n}\n",
    )
    .unwrap();
    let app = root.join("app.styl");
    fs::write(&app, "").unwrap();
    let mut server = Server::new(Settings {
        resolve: ResolveConfig {
            root: root.clone(),
            aliases: Vec::new(),
        },
        entries: vec![root.join("index.styl")],
        ..Settings::default()
    });

    let properties = complete(&mut server, &app, ".a {\n  disp|\n}\n");
    assert!(properties.contains(&"display".to_string()));
    assert!(properties.contains(&"--primary".to_string()));
    let unclosed = complete(&mut server, &app, ".a {\n  .b {\n    disp|");
    assert!(unclosed.contains(&"display".to_string()));

    let values = complete(&mut server, &app, ".a {\n  display: |\n}\n");
    assert!(values.contains(&"flex".to_string()));
    assert!(values.contains(&"inherit".to_string()));
    assert!(values.contains(&"var(--primary)".to_string()));

    let vars = complete(&mut server, &app, ".a {\n  color: var(--|)\n}\n");
    assert_eq!(vars, ["--primary", "--gap"]);

    let extends = complete(&mut server, &app, ".a {\n  @extend $|\n}\n");
    assert_eq!(extends, ["$panel"]);

    let media = complete(&mut server, &app, "@media (--|) {\n}\n");
    assert_eq!(media, ["--phone"]);

    assert!(complete(&mut server, &app, ".a|\n").is_empty());
    assert!(complete(&mut server, &app, "disp|\n").is_empty());
}
