//! Max free: a different file, and the trait imported under another name.

use impl_scan_attr::register;
use serde::Deserialize;

use crate::Strategy as Pick;

/// Picks the widest block, if it fits.
#[derive(Debug, Default, Deserialize)]
pub struct MaxFreeSlot;

#[register(aliases = ["MFS"], table = "SQUARES")]
impl Pick for MaxFreeSlot {
    fn pick(&self, free: &[u32], width: u32) -> Option<usize> {
        let (index, widest) = free
            .iter()
            .enumerate()
            .max_by_key(|&(i, &w)| (w, std::cmp::Reverse(i)))?;
        (*widest >= width).then_some(index)
    }
}
