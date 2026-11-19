//! Prints what the scanner finds, as JSON, and how long the scan took.
//!
//! ```sh
//! cargo run -p impl-scan --example list_impls -- path/to/src Strategy crate::tables::Table
//! ```
//! Features to evaluate `#[cfg]` with: `IMPL_SCAN_FEATURES=a,b`.

use std::time::Instant;

use impl_scan::{Cfg, Scanner};

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().expect("usage: list_impls <src dir> <trait>...");
    let mut cfg = Cfg::new();
    for feature in std::env::var("IMPL_SCAN_FEATURES")
        .unwrap_or_default()
        .split(',')
        .filter(|f| !f.is_empty())
    {
        cfg = cfg.feature(feature);
    }
    let mut scanner = Scanner::new(&root).cfg(cfg).emit_cargo(false);
    for name in args {
        scanner = scanner.trait_impls(&name);
    }
    let start = Instant::now();
    match scanner.scan() {
        Ok(registry) => {
            let elapsed = start.elapsed();
            println!(
                "{}",
                serde_json::to_string_pretty(&registry).expect("registry serializes")
            );
            for group in registry.group_names() {
                eprintln!("{group}: {} impls", registry.group(group).len());
            }
            eprintln!("scanned {root} in {:.1} ms", elapsed.as_secs_f64() * 1000.0);
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
