# stylet in WebStorm and other JetBrains IDEs

There's no stylet plugin for JetBrains IDEs yet, but two generic features get you
highlighting and the full language server (errors, formatting, completions, go to
definition and references): TextMate bundles and the LSP4IJ plugin.

## 1. Highlighting

JetBrains IDEs read the grammar of a VS Code extension directly.

1. Get the extension folder: either `editors/vscode` from a checkout of this
   repository, or a VSIX from the [releases](https://github.com/tutory/stylet/releases)
   (`stylet-universal.vsix`), unzipped; use its `extension/` folder.
2. *Settings → Editor → TextMate Bundles → +* and select that folder.

## 2. `.styl` files: stylet instead of Stylus

WebStorm's bundled Stylus plugin claims `*.styl`. In projects that use stylet, either

- disable it (*Settings → Plugins → Installed → Stylus*), or
- in *Settings → Editor → File Types*, remove `*.styl` from "Stylus" (the TextMate
  bundle then handles it).

## 3. Language server

1. Install the free [LSP4IJ](https://plugins.jetbrains.com/plugin/23257-lsp4ij) plugin
   (by Red Hat) and restart.
2. *Settings → Languages & Frameworks → Language Servers → +* ("New Language Server"):
   - **Name:** `stylet`
   - **Command:** the stylet binary followed by `lsp`:
     - with the npm package in the project (recommended, uses the project's version):
       `$PROJECT_DIR$/node_modules/.bin/stylet lsp`
       (on Windows: `$PROJECT_DIR$\node_modules\.bin\stylet.cmd lsp`)
     - with stylet on your `PATH`: `stylet lsp`
   - **Mappings → File name patterns:** `*.styl`, language id `stylet`
3. Open a `.styl` file. The *Language Servers* tool window shows whether the server
   runs, and can log its traffic for troubleshooting.

The server reads `stylet.toml` from its working directory, which is the project root.
It compiles the configured entries as you type, so errors show up in the context of the
files that import them.

What you get:

- errors as you type
- *Code → Reformat Code* with the `[fmt]` settings of `stylet.toml`; for format on save,
  enable *Settings → Tools → Actions on Save → Reformat code*
- completions: CSS properties and their keywords, the project's custom properties
  (`var(--…)`), placeholders after `@extend`, `@custom-media` names
- go to definition and find usages for `$placeholders`, custom properties and custom
  media; go to definition for `@import` paths
- the structure view (rules, placeholders, at-rules)

## Updating

The language server is the stylet binary itself: updating `@tutory_de/stylet` in the
project updates it. Restart it from the *Language Servers* tool window after an update.
The TextMate bundle only needs updating when the grammar changes.
