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
| `-w, --watch` | Rebuild when an input changes |

Output isn't written when there are errors, so `--watch` keeps the last good build.

## `stylet fmt`

```sh
stylet fmt                 # every .styl file below the current directory
stylet fmt src other.styl  # files and directories
stylet fmt --check         # list unformatted files, exit 1 if there are any
stylet fmt -               # stdin to stdout (for editors)
```

The formatter keeps comments and up to one blank line, puts every statement on its
own line, keeps line breaks inside values as continuation lines and removes
semicolons. It refuses files with syntax errors.

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

[[entry]]
input = "client/index.styl"
output = "public/dist/index.css"

[[entry]]
input = "client/admin/index.styl"
output = "public/dist/admin.css"

[fmt]
indent = 2              # spaces, or "tab"
sort_properties = false # shorthands before longhands, otherwise alphabetical
nested_blocks_last = false # nested rules/at-rules below the declarations
align_strings = true    # align continuation lines of strings (grid-template-areas)
```

Command-line flags add to the `[build]` settings.
