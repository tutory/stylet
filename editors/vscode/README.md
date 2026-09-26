# stylet for VS Code

Language support for [stylet](https://tutory.github.io/stylet/): syntax highlighting,
errors as you type, formatting (with the project's `stylet.toml` settings), go to
definition for `@import` paths and `$placeholders`, placeholder references, an
outline of rules and placeholders, and completions: CSS properties and their
keywords, the project's custom properties (`var(--…)`), placeholders after
`@extend` and `@custom-media` names.

## Requirements

The `stylet` binary, on your `PATH` or configured with `stylet.path`. The extension
starts `stylet lsp` in the workspace folder, so `stylet.toml` (entries, aliases,
`[fmt]`) is picked up; the server restarts when `stylet.toml` changes.

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
| `stylet.path` | `stylet` | Path to the stylet binary |
| `stylet.trace.server` | `off` | Log the language server communication |

Format on save: `"[stylet]": { "editor.formatOnSave": true }`.
