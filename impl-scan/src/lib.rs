//! A build-time registry for Rust.
//!
//! Rust has no runtime reflection, so "every type that implements `Strategy`" cannot
//! be asked for while the program runs. impl-scan asks it once, in `build.rs`: it parses
//! the crate's sources with [`syn`], collects the implementations of the traits you name,
//! and renders Handlebars templates that spell out, for every implementation, the code
//! you would otherwise write by hand: an enum of all variants, typed storage, a
//! `match` that hands out `&dyn Strategy`. The result is ordinary static code that the
//! compiler checks, with no runtime registry and no dynamic loading.
//!
//! ```no_run
//! // build.rs
//! use impl_scan::{Renderer, Scanner};
//!
//! fn main() {
//!     let registry = Scanner::new("src")
//!         .trait_impls("Strategy").group("strategies")
//!         .scan()
//!         .unwrap_or_else(|e| panic!("{e}"));
//!     let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("registry.rs");
//!     Renderer::new()
//!         .render_to_file("templates/registry.hbs", &registry, out)
//!         .unwrap_or_else(|e| panic!("{e}"));
//! }
//! ```
//!
//! See the repository README for the template data, the built-in partials and the
//! limits of the scan.

mod case;
mod cfg;
pub mod checks;
mod error;
mod render;
mod scan;

pub use cfg::Cfg;
pub use error::Error;
pub use render::{Renderer, BUILTIN_PARTIALS};
pub use scan::{Item, Registry, Scanner};
