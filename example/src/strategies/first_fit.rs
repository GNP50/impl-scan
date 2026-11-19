//! First fit.

use impl_scan_attr::register;
use serde::Deserialize;

use crate::Strategy;

/// Picks the first block that fits.
#[derive(Debug, Default, Deserialize)]
pub struct FirstFit {
    /// Blocks narrower than this are never picked.
    #[serde(default)]
    pub min_width: u32,
}

#[register(name = "FIRST_FIT", aliases = ["FF"], table = "LINEAR")]
impl Strategy for FirstFit {
    fn pick(&self, free: &[u32], width: u32) -> Option<usize> {
        free.iter().position(|&w| w >= width.max(self.min_width))
    }
}
