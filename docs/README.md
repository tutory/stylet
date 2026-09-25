# stylet documentation

- [The language](language.md): syntax, nesting, imports, placeholders, `@custom-media`, URLs
- [Command line and configuration](cli.md): `build`, `fmt`, `stylet.toml`
- [Migrating from Stylus](migrate.md): `stylet migrate`, options and warnings
- [Playground](https://tutory.github.io/stylet/playground/): try stylet in the browser (locally: `playground/build.sh`, then serve `playground/`)
- Syntax highlighting on the web: [`playground/stylet-hljs.js`](../playground/stylet-hljs.js) is a
  [highlight.js](https://highlightjs.org) language definition:
  `hljs.registerLanguage('stylet', stylet)` (alias `styl`).
