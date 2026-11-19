# impl-scan-attr

The `#[register]` attribute for [impl-scan](https://github.com/GNP50/impl-scan). It leaves
the item unchanged and only checks that its arguments are literals. The impl-scan build
script reads them as the registered name, aliases and metadata.

```rust
use impl_scan_attr::register;

#[register(name = "MAX_FREE_SLOT", aliases = ["MFS"], table = "SQUARES")]
impl Strategy for MaxFreeSlot { /* ... */ }
```

Licensed under either of Apache-2.0 or MIT at your option.
