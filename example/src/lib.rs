//! A runnable example of impl-scan.
//!
//! Strategies and tables live in their own files and only implement a trait.
//! `build.rs` finds them and generates `StrategyKind`, `TableKind`, the `Strategies`
//! and `Tables` stores and `default_strategy`; nothing here lists them by hand.

pub mod strategies;
pub mod tables;

/// Chooses a slot among the free ones.
pub trait Strategy {
    /// The index of the chosen slot in `free`, if any fits `width`.
    fn pick(&self, free: &[u32], width: u32) -> Option<usize>;
}

/// A lookup table a strategy can read.
pub trait Table {
    /// The value stored for `key`.
    fn lookup(&self, key: u32) -> u32;
}

include!(concat!(env!("OUT_DIR"), "/registry.rs"));
