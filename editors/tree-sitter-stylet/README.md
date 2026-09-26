# tree-sitter-stylet

A [tree-sitter](https://tree-sitter.github.io) grammar for stylet, with highlight
queries (`queries/highlights.scm`) for Neovim, Zed, Helix and other tree-sitter editors.

Line breaks end declarations; `src/scanner.c` emits that terminator unless the next
line continues the value (it starts with a string, e.g. `grid-template-areas`) or
opens a block. After commas and inside brackets values continue automatically.

```sh
npx tree-sitter-cli generate   # after editing grammar.js
npx tree-sitter-cli test       # test/corpus
npx tree-sitter-cli parse file.styl
```
