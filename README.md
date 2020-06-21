Light INI parser
================

![Rust](https://github.com/lparcq/light-ini/workflows/Rust/badge.svg)

This library implements an event-driven parser for the [INI file format](https://en.wikipedia.org/wiki/INI_file).

It doesn't load data in a container. It's an alternative to [rust-ini](https://crates.io/crates/rust-ini)
that avoids building an intermediate hash map if it's not needed.

```toml
[dependencies]
light_ini = "0.1"
```

See the documentation and examples for details.

## Format

- There is no limitation in the names of the properties.

- Comments are only allowed in their own line.

- There is no escape or quoting characters

## License

Licensed under [MIT license](LICENSE-MIT).
