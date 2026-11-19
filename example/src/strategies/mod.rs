//! The strategies. Each one is found by the scanner, wherever it is written.

pub mod first_fit;
pub mod max_free;

/// Strategies compiled only with `--features experimental`.
#[cfg(feature = "experimental")]
pub mod experimental {
    use serde::Deserialize;

    /// Picks the last block that fits.
    #[derive(Debug, Default, Deserialize)]
    pub struct LastFit;

    impl crate::Strategy for LastFit {
        fn pick(&self, free: &[u32], width: u32) -> Option<usize> {
            free.iter().rposition(|&w| w >= width)
        }
    }
}
