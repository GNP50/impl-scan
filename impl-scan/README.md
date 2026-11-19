# impl-scan

Build-time registry for Rust: in `build.rs`, find every implementation of a trait with
`syn` and render Handlebars templates into static code (enums, typed stores, `&dyn Trait`
dispatch).

```rust
// build.rs
let registry = impl_scan::Scanner::new("src")
    .trait_impls("crate::Strategy").group("strategies")
    .scan()
    .unwrap_or_else(|e| panic!("\n{e}\n"));
```

See the [repository README](https://github.com/GNP50/impl-scan) for the template data, the
built-in partials (`registry_enum`, `store`, `dispatch`), the checks and the limits.

Licensed under either of Apache-2.0 or MIT at your option.
