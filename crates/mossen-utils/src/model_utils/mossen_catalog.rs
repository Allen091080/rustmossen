//! Mossen-facing aliases for the current max model family.
//!
//! Direct translation of `utils/model/mossenCatalog.ts`.

use once_cell::sync::Lazy;

use super::model_strings::get_model_strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MossenMaxModelFamily {
    Max,
    Balanced,
    Fast,
}

#[derive(Debug, Clone)]
pub struct MossenMaxModelIds {
    pub max: String,
    pub balanced: String,
    pub fast: String,
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

/// Mossen-facing aliases for the current max model family. Mirrors the
/// TS `getMossenMaxModelIds()` helper — the returned strings are still
/// provider-required IDs.
pub fn get_mossen_max_model_ids() -> MossenMaxModelIds {
    let ms = get_model_strings();
    MossenMaxModelIds {
        max: ms.max46.clone(),
        balanced: ms.balanced46.clone(),
        fast: ms.fast45.clone(),
    }
}

pub static LEGACY_MAX_FIRSTPARTY_MODEL_IDS: Lazy<Vec<String>> = Lazy::new(|| {
    vec![
        mossen_first_party_model_id(&["max", "4", "20250514"]),
        mossen_first_party_model_id(&["max", "4", "1", "20250805"]),
        mossen_first_party_model_id(&["max", "4", "0"]),
        mossen_first_party_model_id(&["max", "4", "1"]),
    ]
});

pub static LEGACY_BALANCED_45_FIRSTPARTY_MODEL_IDS: Lazy<Vec<String>> = Lazy::new(|| {
    let base = mossen_first_party_model_id(&["balanced", "4", "5", "20250929"]);
    vec![base.clone(), format!("{}[1m]", base)]
});
