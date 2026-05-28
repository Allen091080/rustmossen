//! I18nKey — translated from utils/i18n/keys.ts.
//!
//! In TypeScript this file derives `I18nKey = keyof typeof STRINGS_EN` from the
//! English source-of-truth dictionary. In Rust we have no compile-time literal
//! key enumeration; instead we expose `I18nKey` as a string alias and provide a
//! runtime helper that returns the authoritative key set from `STRINGS_EN`.
//!
//! Dependency direction (must remain one-way; circular import is a hard error):
//!   strings_en.rs -> keys.rs -> strings_zh.rs -> index.rs
//!
//! Do NOT import `keys.rs` from `strings_en.rs`. STRINGS_ZH uses the runtime
//! parity check in [`crate::i18n::get_missing_i18n_keys`] to enforce that every
//! EN key has a ZH counterpart.

use crate::i18n::strings_en::STRINGS_EN;

/// All valid i18n keys live in the EN table; this alias mirrors
/// `keyof typeof STRINGS_EN` in the TS source.
pub type I18nKey = &'static str;

/// Returns the authoritative set of i18n keys, derived from `STRINGS_EN`.
pub fn all_keys() -> Vec<I18nKey> {
    STRINGS_EN.keys().copied().collect()
}
