# stylet — plan

Open-source successor to Stylus with deliberately reduced functionality. Written in Rust.
License: MIT OR Apache-2.0. File extension: `.styl`.

## Language

Required `{}` blocks, required `prop: value` colons, no semicolons (a newline ends a declaration).

```styl
// line comment, dropped from output
/* block comment, kept (stripped when minifying, except /*! */) */
@import 'base/reset'
@import '@/components/button'

$btn {
  padding: 4px 8px
  border: 1px solid currentColor
}

.button {
  @extend $btn
  color: red

  &:hover {
    color: blue
  }

  @media (min-width: 600px) {
    padding: 8px
  }
}
```

Output (nesting is **not** flattened; native CSS nesting is emitted):

```css
.button {
  padding: 4px 8px;
  border: 1px solid currentColor;
}
.button {
  color: red;
  &:hover {
    color: blue;
  }
  @media (min-width: 600px) {
    padding: 8px;
  }
}
```

### In scope
- Nesting, `&`, nested at-rules — emitted as native CSS nesting
- `@import` — inlined, imported **once** (dedupe), path **aliases** (e.g. `@/`), cycle detection.
  A leading `/` resolves against the configured project `root` (webapp uses `@import '/client/variables'`).
  Importing a directory resolves to its `index.styl`. `.css` files are inlined as-is.
  `@import` may be nested in rules and at-rules; the imported file's contents are placed at that position.
  - **Dedupe per context**: a file is included once per (canonical path, chain of enclosing rules/at-rules).
    Importing it twice under `.a` emits it once; under `.a` and `.b` it is emitted (scoped) twice.
  - Files that **define placeholders** must be imported at the top level or inside `@layer`s only (error otherwise).
  - Non-nestable at-rules (`@font-face`, `@keyframes`, `@property`, `@counter-style`, …) in a file imported
    inside a style rule are an error.
  - A top-level `&` is only allowed in files imported inside a style rule.
  - Preferred form for cascade layers: `@import 'x' layer(name)` (CSS syntax), compiles to `@layer name { … }`.
- `@extend` — **placeholders only** (`$name`), across files. `@extends` is accepted as an alias
  (webapp uses it 440×); the formatter normalizes to `@extend`.
- `@custom-media` (Media Queries Level 5), shared across imports. Config `resolve_custom_media`
  (default **false**): false passes `@custom-media` and `(--name)` through as-is (native support or a
  downstream tool handles it); true substitutes the conditions at compile time and drops the definitions,
  erroring on unknown names.
- Comments: `//` dropped, `/* */` kept
- Everything else (values, function calls, unknown at-rules) passes through verbatim as CSS

### Out of scope (v1)
Variables, mixins, `if`/`for`, `{}` interpolation, math, built-in functions, **custom functions**
(CSS itself is gaining these; revisit later — a Rust library API, optionally WASM plugins, would slot in).

### Parsing rules
- A line ending with `{` opens a rule/at-rule block; otherwise a statement is `prop: value` or an `@`-statement.
- A value continues onto the next line while `()`/`[]` are unbalanced, the line ends with `,`,
  or the next line is more indented and starts with a string (`grid-template-areas: "a b"` ⏎ `  "c d"`).

### Placeholder / @extend semantics
- `$name { … }` defines a placeholder; allowed at the top level (or inside `@layer`s). Unused placeholders emit nothing.
- `@extend $name` (any depth, any file after import) adds the extender's **resolved** selector to the
  placeholder's selector list. Nested extenders resolve through their parents, using `:is()` for parent lists:
  `.a, .b { .c { @extend $x } }` → `:is(.a, .b) .c`.
- The placeholder rule is emitted where it was defined, preserving cascade order. A placeholder may be
  defined (or imported) more than once; like in Stylus, an `@extend` reaches the copies emitted before it,
  or the first copy if none precedes it.
- `@extend` inside nested rules of a placeholder extends with the placeholder's selectors nested accordingly.
- `@extend` of an unknown placeholder is an error; `@extend` of a non-placeholder selector is an error.

## Bundler
- Single entry → single CSS file; multiple entries via config / CLI args share the parse cache.
- **Source maps** (v3) back to original `.styl` files.
- **`url()` rewriting**: relative URLs in imported files are rebased relative to the output file.
- **Asset inlining** without a function: `url('icon.svg?inline')` is replaced by a data URI
  (replaces Stylus `embedurl()`, used 47× in webapp). Later: content hashing.
- Optional minified output.
- `--watch` mode.

## Tooling
- **Formatter** (`stylet fmt`, `--check`): few options — indent width, tabs vs spaces.
  Optional property sorting (off by default, config switch): shorthand before its longhands
  (`margin` before `margin-top`), otherwise alphabetical; repeated same-property fallbacks keep their relative order.
  Needs a built-in shorthand→longhand table.
- **Migration** (`stylet migrate`): see [Migration](#migration).
- **Highlighting**: VS Code extension (TextMate grammar + LSP client), tree-sitter grammar, HTML highlight library.
  Note: `.styl` collides with existing Stylus extensions/grammars.
- **LSP**: diagnostics, go-to-import, placeholder definition/references, format on save.
- **Docs + playground**: static docs site; playground compiles/formats a single string in the browser via WASM
  (no files, no `@import`).

## Migration

`stylet migrate` rewrites `.styl` files in place. It uses a lenient parser for Stylus's indentation-based syntax
and a **static evaluator**: whatever can be resolved at migration time is inlined, and everything else gets a warning.

Corpus: `../webapp`, 550 `.styl` files (excluding `.claude/worktrees`), 8 env entry trees × 3 entries
(`index`, `digital`, `admin`). Currently built with `stylus` + `axis` + custom JS functions, then autoprefixer + esbuild minify.

| Construct | Webapp usage | Migration |
|---|---|---|
| Syntax (indentation, optional colons) | nearly all files | rewritten to braces + colons, no semicolons |
| `$placeholder` + `@extends $x` | ~400 | kept (`@extend $x`) |
| `@extends .real-selector` | 41 | **warn**; suggest turning the target into a placeholder |
| Variables (`x = …`, `$x = …`) | 138 top-level names, ~90 local | → **CSS custom properties** (see below); local vars inlined |
| Mixins (local + `axis`: `bold`, `semibold`, `rounded`, `flexify`, `clr`, …) | ~900 calls | **inlined** (body expanded, args substituted); warn if the body uses loops or conditions |
| Math / color built-ins on literal values (`lighten`, `rgba($c, .5)`, `10px * 2`) | some | **evaluated** to a literal; warn if it can't be |
| `for … in` loops + `{}` interpolation | 24 loops, 85 interpolations | **warn** (candidates for CSS custom properties); original kept as `// stylet-migrate:` comment |
| `if` / `unless` (incl. `isDevelopment`) | 13 | **warn** |
| Custom JS functions (`toPx`, `toPxFloored`) | 6 | **warn**; suggest precomputing or using `calc()` |
| `embedurl()` | 47 | → `url('…?inline')` |
| Property lookup (`@height`, …) | ~30 | **warn** |
| `@import` nested in `@layer` | `client/index.styl` | → top-level `@import '…' layer(name)` |
| `@import` nested in a selector | ~160 in 23 files | kept; warn only if a rule above is violated (placeholders, non-nestable at-rules) |
| `@css { }` literal blocks | 12 | contents inlined verbatim |
| `@media`, `@layer`, `@container`, `@font-face`, `@keyframes`, `@supports`, `@property` | many | passthrough |

### Variables → custom properties

`--vars=props` (default) or `--vars=inline`; `--var-prefix=<p>` (e.g. `s-`) for all generated names.

- Top-level `foo = 4px` / `$foo = 4px` → `:root { --foo: 4px }` at the same place in the file; declaration-value
  usages become `var(--foo)`. Names keep their case (`$sidebarTabsWidth` → `--sidebarTabsWidth`).
- Usages where custom properties cannot work are **inlined as literals**: `@media`/`@container` preludes
  (164 breakpoint usages in webapp), selectors, interpolation, placeholder/loop/condition contexts.
- Opt-in `--custom-media`: vars used in `@media` preludes become `@custom-media --name (…)` definitions and
  usages become `(--name)` (requires `resolve_custom_media = true` or a downstream tool until browsers ship it).
- Arithmetic on vars → `calc()` (`gap * 2` → `calc(var(--gap) * 2)`). Color built-ins on vars (`lighten(c, 10%)`)
  → evaluated literal with a warning (link to the variable is lost).
- Not converted, inlined instead: local vars (inside rules/mixins), hashes, booleans/conditions, lists used by loops,
  and vars defined in a file that is imported nested.
- Env overrides (vars redefined in `env/*/variables.styl`, 14 names) become a later `:root` declaration, so the
  cascade gives the override. Warn when a var is used before being reassigned (Stylus is order-dependent,
  custom properties are not), and when an overridden var is also inlined (the literal would be env-specific).
- Collisions: the migrator knows every `--name` declared in the corpus (677) and every generated name
  (`$foo` and `foo` both map to `--foo`). A collision is an error that suggests `--var-prefix`.
  Webapp currently has **no** collisions without a prefix.

### Reporting
- Each warning points to file:line and has a category, so `--report` can print a summary table.
- `--dry-run` shows diffs without writing files.
- Verification: build webapp with `stylus` before migrating and with `stylet` after, flatten both
  (e.g. with lightningcss) and diff, ignoring files that still have warnings.
- Because `axis` mixins are inlined, the migration needs an `axis` shim: a vendored copy of the mixins webapp uses, evaluated like local mixins.

## Architecture (Cargo workspace)

| Crate | Responsibility |
|---|---|
| `stylet-syntax` | Lexer + error-recovering parser → lossless CST (`rowan`). Shared by everything. |
| `stylet-resolve` | Virtual file system trait, import resolution, aliases, dedupe, cycle detection. |
| `stylet-compile` | Placeholders/`@extend`, selector resolution, `url()` rebasing, CSS emit, source maps, minify. |
| `stylet-fmt` | Formatter + optional property sort. |
| `stylet-migrate` | Stylus-subset → stylet converter. |
| `stylet-highlight` | CST → highlighted HTML. |
| `stylet-cli` | `build`, `watch`, `fmt`, `check`, `migrate`. Config: `stylet.toml` (entries, out dir, root, aliases, `resolve_custom_media`, fmt options). |
| `stylet-lsp` | Language server (`tower-lsp`). |
| `stylet-wasm` | `compile(src)`, `format(src)`, `highlight(src)` for the playground. |
| `editors/vscode`, `tree-sitter-stylet`, `docs/` | Editor integration, grammar, docs + playground. |

Core crates never touch the real filesystem directly (VFS trait) so they build for `wasm32`.

## Milestones
1. **Syntax** — lexer, CST, parser with error recovery; snapshot tests.
2. **Compile core** — emit CSS with native nesting, comments, passthrough.
3. **Imports + bundling** — resolve, aliases, dedupe, source maps, `url()` rebasing, CLI `build`/`watch`, `stylet.toml`.
4. **Placeholders + `@extend`**.
5. **Formatter** — idempotent; property sort.
6. **Migration** — lenient Stylus parser, static evaluator (vars, mixins, literal math/colors), axis shim,
   warnings report; verified against webapp (see Migration).
7. **WASM + playground + docs**.
8. **Editors** — TextMate grammar + VS Code extension, LSP, tree-sitter grammar, HTML highlighter.
9. **Release** — prebuilt binaries (GitHub releases, `cargo binstall`, Homebrew), crates.io.

## Project
- Repo: https://github.com/tutory/stylet
- Test corpus: `../webapp`

## Post-processing
- v1: stylet emits plain CSS; its minify only strips whitespace/comments. Webapp keeps autoprefixer + esbuild.
- Later: optional `lightningcss` cargo feature (off by default) with `--targets` for prefixing, minification and
  lowering nesting for older browsers, keeping source maps intact.
- Native nesting output needs browsers from ~2023 on; older targets must lower it (esbuild `target` or lightningcss).
