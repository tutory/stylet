# Command line and configuration

## `stylet build`

```sh
stylet build src/index.styl -o dist/index.css   # one file
stylet build src/index.styl                      # to stdout
stylet build                                     # all [[entry]]s of stylet.toml
stylet build --watch                             # rebuild on changes
```

| Option | |
|---|---|
| `-o, --output <file>` | Output file (single input only) |
| `--minify` | Strip whitespace and comments |
| `--source-map` | Write `<output>.map` and a `sourceMappingURL` comment |
| `--resolve-custom-media` | Substitute `@custom-media` references |
| `--flatten` | Compile nesting away, for browsers without CSS nesting |
| `-w, --watch` | Rebuild when an input changes |

Output isn't written when there are errors, so `--watch` keeps the last good build.

## `stylet fmt`

```sh
stylet fmt                 # every .styl file below the current directory
stylet fmt src other.styl  # files and directories
stylet fmt --check         # list unformatted files, exit 1 if there are any
stylet fmt -               # stdin to stdout (for editors)
```

The formatter is opinionated: every rule below is on by default and can be switched
off in `stylet.toml`. It keeps comments and up to one blank line, puts every statement
on its own line, keeps line breaks inside values as continuation lines, removes
semicolons, adds blank lines around blocks and `@import` groups, puts one selector
per line, normalizes spacing around commas and parentheses, writes strings in single
quotes and fractions without a leading zero, and sorts properties (shorthands before
their longhands, otherwise alphabetical). A shorthand that follows one of its
longhands (and so overrides it) stays after it. It refuses files
with syntax errors.

Continuation lines that start with a string are aligned to the first string, so
`grid-template-areas` reads as a grid:

```styl
grid-template-areas: "header header"
                     "sidebar main"
```

With `nested_blocks_last`, nested rules and at-rules with a block move below the
declarations of their block (comments directly above a block move with it,
`@extend` stays first). This changes the output order, which matters only if a
nested rule and a declaration of its parent apply to the same element — e.g. with
`& { … }`.

## `stylet migrate`

See [Migrating from Stylus](migrate.md).

## `stylet lsp`

A language server on stdin/stdout, started in the project directory (it reads
`stylet.toml` from there). It compiles the configured entries as you type, with
unsaved editor contents taking precedence, and offers:

- diagnostics for every file the entries reach (open files outside them get syntax errors)
- formatting with the `[fmt]` settings
- go to definition and references for `$placeholders`, custom properties (`var(--x)` →
  `--x: …` and `@property --x`) and custom media (`(--phone)` → `@custom-media`);
  go to definition for `@import` paths
- document symbols (rules, placeholders, at-rules)
- completions: CSS properties and their keywords (from [webref](https://github.com/w3c/webref)),
  custom properties declared in the project (`var(--…)`), placeholders after `@extend`
  and `@custom-media` names in media queries

The [VS Code extension](../editors/vscode) starts it for you. Other editors run
`stylet lsp` for `.styl` files, e.g. in Neovim:

```lua
vim.lsp.config('stylet', { cmd = { 'stylet', 'lsp' }, filetypes = { 'stylet' }, root_markers = { 'stylet.toml' } })
vim.lsp.enable('stylet')
```

## `stylet.toml`

stylet looks for `stylet.toml` in the current directory and its parents
(or use `--config <path>`). All keys are optional.

```toml
# Directory that `/`-prefixed imports resolve against (relative to this file).
root = "."

[aliases]
"@/" = "client"

[build]
minify = false
source_map = true
resolve_custom_media = false
flatten = false        # compile nesting away (browsers before 2023)

[[entry]]
input = "client/index.styl"
output = "public/dist/index.css"

[[entry]]
input = "client/admin/index.styl"
output = "public/dist/admin.css"

[fmt]
indent = 2                        # spaces, or "tab"
sort_properties = true            # shorthands before longhands, otherwise alphabetical
nested_blocks_last = false        # nested rules/at-rules below the declarations
align_strings = true              # align continuation lines of strings (grid-template-areas)
blank_lines_around_blocks = true  # blank line before and after every block
blank_lines_around_imports = true # blank line around each group of @imports
selector_per_line = true          # one selector per line in selector lists
normalize_spacing = true          # `a, b`, `(a)`: space after commas, none inside parentheses
single_quotes = true              # 'strings' (unless they contain a single quote)
leading_zero = false              # .5 instead of 0.5
```

Command-line flags add to the `[build]` settings.
