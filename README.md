Light INI parser
================

![Rust](https://github.com/lparcq/light-ini/workflows/Rust/badge.svg)

This library implements an event-driven parser for the [INI file format](https://en.wikipedia.org/wiki/INI_file).

It doesn't load data in a container. It's an alternative to [rust-ini](https://crates.io/crates/rust-ini)
that avoids building an intermediate hash map if it's not necessary.

```toml
[dependencies]
light_ini = "0.3"
```

See the [documentation](https://docs.rs/light-ini) and examples for details.

## Format

- There is no limitation in the names of the properties.

- Comments are only allowed in their own line. The default character to start a comment is `;`.
  Use `IniParser::with_start_comment` to use a different character such as `#`.

- There is no escape or quoting characters

## License

Licensed under [MIT license](LICENSE-MIT).
