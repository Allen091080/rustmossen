//! Mossen-facing aliases for the current frontier model family.
//!
//! Direct translation of `utils/model/mossenCatalog.ts`.

use once_cell::sync::Lazy;

use super::model_strings::get_model_strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MossenFrontierModelFamily {
    Opus,
    Sonnet,
    Haiku,
}

#[derive(Debug, Clone)]
pub struct MossenFrontierModelIds {
    pub opus: String,
    pub sonnet: String,
    pub haiku: String,
}

const MOSSEN_FIRST_PARTY_MODEL_PREFIX: &str = "mossen";

fn mossen_first_party_model_id(parts: &[&str]) -> String {
    let mut all = Vec::with_capacity(parts.len() + 1);
    all.push(MOSSEN_FIRST_PARTY_MODEL_PREFIX.to_string());
    for p in parts {
        all.push((*p).to_string());
    }
    all.join("-")
}

/// Mossen-facing aliases for the current frontier model family. Mirrors the
/// TS `getMossenFrontierModelIds()` helper — the returned strings are still
/// provider-required IDs.
pub fn get_mossen_frontier_model_ids() -> MossenFrontierModelIds {
    let ms = get_model_strings();
    MossenFrontierModelIds {
        opus: ms.opus46.clone(),
        sonnet: ms.sonnet46.clone(),
        haiku: ms.haiku45.clone(),
    }
}

pub static LEGACY_OPUS_FIRSTPARTY_MODEL_IDS: Lazy<Vec<String>> = Lazy::new(|| {
    vec![
        mossen_first_party_model_id(&["opus", "4", "20250514"]),
        mossen_first_party_model_id(&["opus", "4", "1", "20250805"]),
        mossen_first_party_model_id(&["opus", "4", "0"]),
        mossen_first_party_model_id(&["opus", "4", "1"]),
    ]
});

pub static LEGACY_SONNET_45_FIRSTPARTY_MODEL_IDS: Lazy<Vec<String>> = Lazy::new(|| {
    let base = mossen_first_party_model_id(&["sonnet", "4", "5", "20250929"]);
    vec![base.clone(), format!("{}[1m]", base)]
});
