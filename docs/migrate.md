# Migrating from Stylus

`stylet migrate` converts Stylus sources to stylet. It evaluates Stylus the way the
Stylus compiler does — per entry, in import order — and writes a stylet version of
every file it reaches.

```sh
stylet migrate env/tutory/client/index.styl env/tutory/client/digital/index.styl \
  --preload node_modules/axis/axis/index.styl \
  --define isDevelopment=false \
  --dry-run
```

Drop `--dry-run` to rewrite the files in place, or pass `--out <dir>` to write the
converted files to a copy.

## What is converted

| Stylus | stylet |
|---|---|
| Indentation, optional colons | `{}` blocks, `prop: value`, no semicolons |
| Mixins (incl. property-style calls like `line-clamp: 1`) | inlined, with conditions on their arguments decided |
| Functions, `s()`, `%`, `unquote()`, arithmetic, color functions on literals | evaluated |
| Global variables | custom properties in `:root` (arithmetic becomes `calc()`); `--vars inline` inlines them |
| Variables in media conditions | inlined, or `@custom-media` with `--custom-media` |
| Local variables, hashes, `{$block}` interpolation, `@prop` lookups | inlined |
| `$placeholder` / `@extend(s) $placeholder` | kept; nested placeholders move to the top level |
| `/selector` root selectors, nested `@keyframes` | moved to the top level of the file |
| `embedurl('x.svg')` | `url('x.svg?inline')` |
| `@import` inside `@layer` | `@import '…' layer(name)` |
| Imports Stylus finds through its lookup paths | rewritten to root-relative paths |
| `@css { … }` | inlined |

Stylus scoping rules are followed: mixins and variables defined in a file imported
inside a block are only visible in that block, and an `@extend` only reaches copies of
a placeholder emitted before it.

## Options

| Option | |
|---|---|
| `--preload <file>` | Evaluate a mixin library (e.g. axis) for its definitions only |
| `--define name=value` | Value for something Stylus got from JS (e.g. `isDevelopment`) |
| `--vars props\|inline` | Global variables as custom properties (default) or inlined |
| `--var-prefix <p>` | Prefix for generated custom properties (`--<p>name`) |
| `--custom-media` | Variables in media conditions become `@custom-media` (build with `resolve_custom_media = true`) |
| `--unroll-loops` | Unroll `for` loops instead of commenting them out |
| `--out <dir>`, `--dry-run`, `-q` | Where to write, report only, summary only |

## Warnings

Whatever can't be converted statically is kept as `// stylet-migrate:` comments next
to a warning with file and line. Categories:

| Category | Meaning / what to do |
|---|---|
| `extend-selector` | `@extend` of a real selector: turn the target into a `$placeholder` |
| `loop` | `for` loop: use custom properties, or `--unroll-loops` |
| `condition` | condition on a value Stylus got from JS: `--define` it |
| `js-function`, `js-plugin` | functions from JS plugins (`use()`, `define()`): replace them |
| `used-before-defined` | a function called before its definition; Stylus output it literally (usually a bug) |
| `units` | mixed-unit arithmetic: Stylus ignored units; stylet keeps a correct `calc()` |
| `placeholder` | placeholder in a file imported inside a rule: its `@extend`s were inlined |
| `import-dependent` | a file converts differently depending on where it's imported; the last version is kept |
| `url` | relative `url()`: Stylus left it relative to the output, stylet rebases it from the source file |
| `asset`, `import` | a file that couldn't be found |

Name collisions between generated custom properties and existing ones stop the
migration; resolve them with `--var-prefix` or by renaming.

## Verifying

Compile the converted sources with `stylet build` and compare the result with the
Stylus output. For webapp all 24 entries (8 envs × index/digital/admin) match within
0.6% of declarations (after removing `@extend`s of real selectors on the Stylus side);
the remaining differences are JS functions (`toPx`), mixed-unit arithmetic and value
spellings.
