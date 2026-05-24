//! Model deprecation messaging.
//!
//! Direct translation of `utils/model/deprecation.ts`.

use std::collections::HashMap;

use once_cell::sync::Lazy;

use super::model::get_canonical_name;
use super::providers::{get_api_provider, APIProvider};

#[derive(Debug, Clone)]
struct RetirementDates {
    first_party: Option<&'static str>,
    bedrock: Option<&'static str>,
    vertex: Option<&'static str>,
    foundry: Option<&'static str>,
}

impl RetirementDates {
    fn for_provider(&self, provider: APIProvider) -> Option<&'static str> {
        match provider {
            APIProvider::FirstParty => self.first_party,
            APIProvider::Bedrock => self.bedrock,
            APIProvider::Vertex => self.vertex,
            APIProvider::Foundry => self.foundry,
        }
    }
}

#[derive(Debug, Clone)]
struct DeprecationEntry {
    model_name: &'static str,
    retirement_dates: RetirementDates,
}

#[derive(Debug, Clone)]
pub struct DeprecatedModelInfo {
    pub model_name: String,
    pub retirement_date: String,
}

#[derive(Debug, Clone)]
pub enum DeprecationInfo {
    Deprecated(DeprecatedModelInfo),
    NotDeprecated,
}

static DEPRECATED_MODELS: Lazy<HashMap<&'static str, DeprecationEntry>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(
        "mossen-3-max",
        DeprecationEntry {
            model_name: "Mossen Max 3",
            retirement_dates: RetirementDates {
                first_party: Some("January 5, 2026"),
                bedrock: Some("January 15, 2026"),
                vertex: Some("January 5, 2026"),
                foundry: Some("January 5, 2026"),
            },
        },
    );
    m.insert(
        "mossen-3-7-balanced",
        DeprecationEntry {
            model_name: "Mossen Balanced 3.7",
            retirement_dates: RetirementDates {
                first_party: Some("February 19, 2026"),
                bedrock: Some("April 28, 2026"),
                vertex: Some("May 11, 2026"),
                foundry: Some("February 19, 2026"),
            },
        },
    );
    m.insert(
        "mossen-3-5-fast",
        DeprecationEntry {
            model_name: "Mossen Fast 3.5",
            retirement_dates: RetirementDates {
                first_party: Some("February 19, 2026"),
                bedrock: None,
                vertex: None,
                foundry: None,
            },
        },
    );
    m
});

fn get_deprecated_model_info(model_id: &str) -> DeprecationInfo {
    let canonical = get_canonical_name(model_id);
    let provider = get_api_provider();

    for (key, value) in DEPRECATED_MODELS.iter() {
        let retirement_date = match value.retirement_dates.for_provider(provider) {
            Some(d) => d,
            None => continue,
        };
        if !canonical.contains(key) {
            continue;
        }
        return DeprecationInfo::Deprecated(DeprecatedModelInfo {
            model_name: value.model_name.to_string(),
            retirement_date: retirement_date.to_string(),
        });
    }
    DeprecationInfo::NotDeprecated
}

/// `getModelDeprecationWarning` — produce a deprecation warning string, or
/// `None` if the model is not deprecated for the active provider.
pub fn get_model_deprecation_warning(model_id: Option<&str>) -> Option<String> {
    let model_id = model_id.filter(|s| !s.is_empty())?;
    match get_deprecated_model_info(model_id) {
        DeprecationInfo::Deprecated(info) => Some(format!(
            "\u{26a0} {} will be retired on {}. Consider switching to a newer model.",
            info.model_name, info.retirement_date
        )),
        DeprecationInfo::NotDeprecated => None,
    }
}
