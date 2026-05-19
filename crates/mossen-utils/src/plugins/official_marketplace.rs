//! Official marketplace constants.
//!
//! Translated from `utils/plugins/officialMarketplace.ts` (25 lines).

use once_cell::sync::Lazy;

use super::schemas::MarketplaceSource;

/// Source configuration for the official Mossen plugins marketplace.
pub static OFFICIAL_MARKETPLACE_SOURCE: Lazy<MarketplaceSource> = Lazy::new(|| {
    MarketplaceSource::GitHub {
        repo: "mossen/mossen-plugins-official".to_string(),
        git_ref: None,
        path: None,
        sparse_paths: None,
    }
});

/// Display name for the official marketplace.
pub const OFFICIAL_MARKETPLACE_NAME: &str = "mossen-plugins-official";
