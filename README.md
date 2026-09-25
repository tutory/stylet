# stylet

A small, fast successor to [Stylus](https://stylus-lang.com), written in Rust.

stylet keeps the parts of Stylus that modern CSS still lacks and drops the rest:

- `{}` blocks and `prop: value` declarations, **no semicolons**
- nesting and `&`, emitted as **native CSS nesting**
- `@import` bundling (imported once, path aliases, source maps, `url()` rebasing)
- `@extend` for `$placeholder` selectors

No variables, mixins, loops, math or built-in functions — use CSS custom properties and friends instead.

```styl
@import '@/components/button'

$btn {
  padding: 4px 8px
}

.button {
  @extend $btn
  color: red

  &:hover {
    color: blue
  }
}
```

Tooling: formatter, `stylet migrate` for existing Stylus code and a browser playground.
Editor support (VS Code, tree-sitter, language server) is in progress.

```sh
stylet build src/index.styl -o dist/index.css
stylet fmt --check
stylet migrate client/index.styl --dry-run
```

See the [documentation](docs/README.md) and [PLAN.md](PLAN.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project
by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
