# impl-scan

`impl-scan` is a build-time registry for Rust. In `build.rs` it parses your crate with
[`syn`](https://docs.rs/syn), finds every implementation of the traits you name, and
renders [Handlebars](https://docs.rs/handlebars) templates that spell out code for each one:
an enum of every variant, typed storage, a `match` that hands out `&dyn Trait`. You add a
strategy by writing `impl Strategy for MyStrategy` in any file, and the next build wires it
in.

Rust has no runtime reflection, so the program cannot ask "which types implement
`Strategy`?" while it runs. impl-scan asks the question once, before compilation, and turns
the answer into ordinary static code. The compiler checks that code, every `match` is
exhaustive, and nothing is looked up at run time. The cost is a closed world: adding an
implementation means recompiling.

The repository contains the scanner and renderer (`impl-scan`), the inert `#[register]`
attribute (`impl-scan-attr`), and a runnable example (`example/`). Version 0.1.0 is an
early release. Rust 1.85 or newer is required.

## Why not a regex

The idea comes from a network simulator whose `build.rs` looked for
`impl ProvisionableStrategy for (\w+)` with a regex. That works until it doesn't:

| Source | Regex | impl-scan |
| --- | --- | --- |
| `impl<T> Strategy for Foo<T>` | missed | found, generics reported |
| `impl crate::Strategy for Foo`, `impl S for Foo` after `use ... as S` | missed | found, path resolved through `use` |
| `impl  Strategy  for Foo`, `impl` split over lines | missed | found |
| `// impl Strategy for Old`, the same text in a string | **found** | ignored |
| `#[cfg(feature = "x")] impl ...`, `#[cfg(test)] mod tests` | always found | evaluated against the build's features and cfgs |
| two traits named `Key` in different modules | merged | told apart with `trait_impls("crate::a::Key")` |
| a strategy in a **new** file | build script not re-run, stale code | re-run (`rerun-if-changed` on the whole source tree) |

On the simulator's sources (110 files, 44k lines) impl-scan finds the same 7
implementations as the regex. A scan takes about 50 ms with the scanner built in release
mode and about 300 ms in a default debug build of the build script (Apple M-series,
measured with `examples/list_impls.rs`). Only files that mention a queried trait, or a
type found implementing it, are parsed.

## Quick start

```toml
# Cargo.toml
[dependencies]
impl-scan-attr = "0.1"   # optional: only for #[register]

[build-dependencies]
impl-scan = "0.1"
```

```rust
// build.rs
use impl_scan::{checks, Renderer, Scanner};

fn main() {
    let registry = Scanner::new("src")
        .trait_impls("crate::Strategy").group("strategies")
        .trait_impls("crate::Table").group("tables")
        .check(checks::require_doc("strategies"))
        .check(checks::meta_refs("strategies", "table", "tables"))
        .scan()
        .unwrap_or_else(|e| panic!("\n{e}\n"));

    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("registry.rs");
    Renderer::new()
        .partials_dir("templates/partials")
        .and_then(|r| r.render_to_file("templates/registry.hbs", &registry, &out))
        .unwrap_or_else(|e| panic!("\n{e}\n"));
}
```

```rust
// src/lib.rs
include!(concat!(env!("OUT_DIR"), "/registry.rs"));
```

```handlebars
{{!-- templates/registry.hbs --}}
{{> registry_enum enum="StrategyKind" items=strategies}}
{{> store store="Strategies" enum="StrategyKind" trait="crate::Strategy" key="u64" items=strategies serde=true}}
```

That generates `StrategyKind` (with `ALL`, `name()`, `from_name()` and `Display`) and a
`Strategies` store that can be used like this:

```rust
let mut store = Strategies::default();
store.insert_json(StrategyKind::from_name("MFS").unwrap(), 42, r#"{"k": 3}"#)?;
let strategy: &dyn Strategy = store.find(&42).unwrap();
```

The complete version is in [`example/`](example): `cargo test -p impl-scan-example`.

## Registering

Without annotations, every `impl Strategy for X` is found and named after the type in
`SCREAMING_SNAKE_CASE`. `#[register]` adds a stable public name, aliases and free metadata:

```rust
use impl_scan_attr::register;

/// Picks the widest free block.
#[register(name = "MAX_FREE_SLOT", aliases = ["MFS"], table = "SQUARES", weight = 3)]
impl Strategy for MaxFreeSlot { /* ... */ }
```

A public name that is not the type name means you can rename the type without breaking
configuration files that refer to it by name. The attribute can also go on the type
definition instead of the `impl`. At compile time it does nothing but check that its
arguments are literals. The scanner recognises any attribute whose last path segment is
`register`.

## What the templates see

Each group is a list of items sorted by `name`:

```json
{
  "strategies": [{
    "type": "MaxFreeSlot",
    "path": "crate::strategies::max_free::MaxFreeSlot",
    "name": "MAX_FREE_SLOT",
    "aliases": ["MFS"],
    "doc": "Picks the widest free block.",
    "generics": [],
    "meta": { "table": "SQUARES", "weight": 3 },
    "trait": "crate::Strategy",
    "file": "src/strategies/max_free.rs",
    "line": 12
  }]
}
```

`doc` comes from the `impl` block, or else from the type definition. `Renderer::data`
adds your own top-level values. Run the build with `IMPL_SCAN_DUMP=1` to write the
registry to `$OUT_DIR/impl_scan.json`.

Helpers: `snake`, `pascal`, `camel`, `screaming`, `lower`, `upper`, `quote` (a Rust string
literal with escapes), `join list sep` and `doc text fallback=".." indent=N` (one `///` line
per line). HTML escaping is off. Strict mode is on, so a misspelled field fails the build
instead of rendering as an empty string. Output files start with a `// @generated` header
and are written only when their content changes, which keeps their mtime and avoids
needless rebuilds.

## Built-in partials

| Partial | Parameters | Generates |
| --- | --- | --- |
| `registry_enum` | `enum`, `items`, optional `derive` | `enum` with one variant per item, `ALL`, `name()`, `from_name()` (aliases included), `Display` |
| `store` | `store`, `enum`, `trait`, `key`, `items`, optional `serde=true` | A struct holding one `HashMap<key, Type>` per item; typed `x()`, `x_mut()`, `insert_x()`, `remove_x()`, `iter_x()`; `kind_of(id)`, `get(kind, id) -> &dyn Trait`, `get_mut`, `find(id)`, `find_mut`, `remove(id)`, `len`; `insert_json(kind, id, json)` with `serde` |
| `dispatch` | `on`, `enum`, `items`, `body` | A `match` with one arm per item. `body` names a partial, usually inline, that is rendered with the item as context; inside the arm, `T` is the concrete type |

```handlebars
{{#*inline "default_arm"}}Box::new(<T as Default>::default()){{/inline}}
pub fn default_strategy(kind: StrategyKind) -> Box<dyn Strategy> {
    {{> dispatch on="kind" enum="StrategyKind" items=strategies body="default_arm"}}
}
```

In a store, an id lives in at most one map: `insert_x` removes it from the others first,
so `kind_of` and `find` are unambiguous. Your own partials go in a directory passed to
`Renderer::partials_dir`. A partial with the same name as a built-in one replaces it.

## Checks

A check runs on the final registry and fails the build with a located error:

```text
error: `MAX_FREE_SLOT` in group `strategies` references table "SQARES", not found in group `tables`
  --> src/strategies/max_free.rs:12
  = help: known: LINEAR, SQUARES
```

The built-in checks are `checks::require_doc(group)`,
`checks::meta_refs(group, key, target_group)` and `checks::concrete(group)` (no generic
impls). Any `Fn(&Registry) -> Result<(), impl_scan::Error>` works as a check. The scan
always rejects duplicate names or aliases within a group, and a type registered twice.

## Limits

- **Macro-generated impls are invisible.** An `impl` produced by `macro_rules!` or a derive
  is not in the source. Register such types with `Scanner::extra(group, path)`.
- **Name resolution is per file, not the compiler's.** `use` items, aliases, `self`,
  `super` and `crate` are followed. A bare name that is not imported is assumed to be
  defined in the current module, and glob imports are not followed. To tell two
  same-named traits apart, name the trait by path in the `impl` or with an explicit `use`.
- **Module paths come from file paths.** `src/a/b.rs` is `crate::a::b`. `#[path]`
  attributes and `#[cfg]` on a `mod x;` declaration are not followed; a `#![cfg]` inside
  the file is.
- **`cfg(test)` is always false.** A build script cannot know whether the crate is being
  compiled for tests. `cfg_attr` is not evaluated.
- **Generic implementations** are reported with their `generics`, but the built-in
  `store` and `dispatch` need concrete types (see `checks::concrete`).
- **Rebuild cost.** The build script re-runs when any file under the scanned directory
  changes. Add `[profile.dev.build-override] opt-level = 3` if the scan shows up in your
  edit-compile loop.

## Develop and verify

```sh
cargo test --workspace --all-features --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS=-D\ warnings cargo doc --workspace --no-deps --all-features --locked
python scripts/check_packages.py
cargo run -p impl-scan --example list_impls -- path/to/src TraitName
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.
