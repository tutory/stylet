# The stylet language

stylet is CSS with nesting, imports and placeholder extends — and without
semicolons. Everything else (values, functions, at-rules) is plain CSS and is
passed through unchanged.

## Syntax

```styl
// Line comments are dropped from the output.
/* Block comments are kept (and dropped when minifying, except /*! … */). */

.card {
  padding: 1rem
  border: 1px solid var(--border)
  transition: opacity .2s,
    transform .2s

  &:hover {
    opacity: .9
  }

  .title { font-weight: 600 }
}
```

- Blocks use `{ }`; declarations are `property: value`.
- A declaration ends at the end of the line. It continues on the next line while
  brackets are open, after a trailing comma, or when the next line is more
  indented and starts with a string (for `grid-template-areas`).
- Semicolons are an error; `stylet fmt` removes them.

## Nesting

Nesting is emitted as native [CSS nesting](https://developer.mozilla.org/docs/Web/CSS/CSS_nesting)
— stylet does not flatten selectors. `&` refers to the parent selector; `@media`,
`@supports`, `@container` and `@layer` can be nested inside rules.

## Imports

```styl
@import 'reset'                     // ./reset.styl or ./reset/index.styl
@import '/client/variables'         // relative to the project root
@import '@/components/button'       // alias from stylet.toml
@import 'vendor.css'                // inlined as-is
@import 'base' layer(base)          // wrapped in @layer base { … }
```

- A file is included once per context: importing it twice in the same place emits it
  once; importing it inside two different rules emits it in both.
- Imports may be nested in rules; the imported file's top level then belongs to that rule.
- Import cycles are an error.

## Placeholders and `@extend`

```styl
$button {
  border-radius: 4px
  padding: .5rem 1rem
}

.save { @extend $button }
.nav a { @extend $button }
```

compiles to

```css
.save, .nav a {
  border-radius: 4px;
  padding: .5rem 1rem;
}
```

- Placeholders (`$name`) are defined at the top level of a file (or inside `@layer`).
  The rule is emitted where the placeholder is defined; unused placeholders emit nothing.
- Only placeholders can be extended (`@extends` is accepted too). `@extend` can't be
  used inside `@media` and other conditional rules.
- Extenders' selectors are resolved through their nesting (`:is()` for selector lists).
- A placeholder defined or imported more than once receives, in each copy, the
  `@extend`s that follow it (or all of them in the first copy if none follows).

## `@custom-media`

```styl
@custom-media --phone (width <= 600px)

@media (--phone) { … }
```

By default `@custom-media` is passed through. With `resolve_custom_media = true`
(or `--resolve-custom-media`) references are replaced at compile time and the
definitions are dropped; definitions must come before their first use.

## URLs

Relative `url()`s are rebased from the source file to the output file.
`url('icon.svg?inline')` inlines the file as a data URI.
