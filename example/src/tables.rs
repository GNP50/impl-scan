//! Tables.

use impl_scan_attr::register;

use crate::Table;

/// `key -> key`.
#[derive(Debug, Default)]
pub struct Linear;

#[register(name = "LINEAR")]
impl Table for Linear {
    fn lookup(&self, key: u32) -> u32 {
        key
    }
}

/// `key -> key * key`.
#[derive(Debug, Default)]
pub struct Squares;

impl Table for Squares {
    fn lookup(&self, key: u32) -> u32 {
        key * key
    }
}
