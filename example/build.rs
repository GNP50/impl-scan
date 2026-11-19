//! Generates the strategy and table registry into `OUT_DIR/registry.rs`.

use std::path::Path;

use impl_scan::{checks, Renderer, Scanner};

fn main() {
    let registry = Scanner::new("src")
        .trait_impls("crate::Strategy")
        .group("strategies")
        .trait_impls("crate::Table")
        .group("tables")
        .check(checks::concrete("strategies"))
        .check(checks::require_doc("strategies"))
        .check(checks::meta_refs("strategies", "table", "tables"))
        .scan()
        .unwrap_or_else(|e| panic!("\n{e}\n"));

    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("registry.rs");
    Renderer::new()
        .partials_dir("templates/partials")
        .and_then(|r| r.render_to_file("templates/registry.hbs", &registry, &out))
        .unwrap_or_else(|e| panic!("\n{e}\n"));
}
