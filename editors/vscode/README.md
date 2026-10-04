# stylet for VS Code

Language support for [stylet](https://tutory.github.io/stylet/): syntax highlighting,
errors as you type, formatting (with the project's `stylet.toml` settings), go to
definition and references for `$placeholders`, custom properties and custom media
(and go to definition for `@import` paths), an
outline of rules and placeholders, and completions: CSS properties and their
keywords, the project's custom properties (`var(--…)`), placeholders after
`@extend` and `@custom-media` names.

## The stylet binary

The extension starts `stylet lsp` in the workspace folder, so `stylet.toml` (entries,
aliases, `[fmt]`) is picked up; the server restarts when `stylet.toml` changes. It uses,
in this order:

1. the `stylet.path` setting,
2. the project's own version, if it has the npm package `@tutory_de/stylet` installed
   (only in trusted workspaces),
3. the binary bundled with the extension (macOS, Linux, Windows),
4. `stylet` on your `PATH`.

The "stylet" output channel shows which one is used.

## `.styl` and Stylus extensions

stylet files use the `.styl` extension. If a Stylus extension is installed too, tell
VS Code which one to use in the workspace settings:

```json
{
  "files.associations": { "*.styl": "stylet" }
}
```

## Settings

| Setting | Default | |
|---|---|---|
| `stylet.path` | | Path to the stylet binary (empty: see above) |
| `stylet.trace.server` | `off` | Log the language server communication |

Format on save: `"[stylet]": { "editor.formatOnSave": true }`.
