use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use super::schemas::MarketplaceSource;

pub const PLUGIN_MARKETPLACE_ADD_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMarketplaceAddPlan {
    pub token: String,
    pub created_at: u64,
    pub input: String,
    pub source: MarketplaceSource,
    pub source_display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginMarketplaceAddPlanError {
    #[serde(rename = "missing_source")]
    MissingSource,
    #[serde(rename = "invalid_source")]
    InvalidSource { message: String },
    #[serde(rename = "unknown_token")]
    UnknownToken { token: String },
    #[serde(rename = "expired_token")]
    ExpiredToken { token: String },
    #[serde(rename = "add_failed")]
    AddFailed { message: String },
}

#[derive(Debug, Clone)]
pub enum PluginMarketplaceAddPlanResult {
    Ok {
        plan: PluginMarketplaceAddPlan,
    },
    Err {
        error: PluginMarketplaceAddPlanError,
    },
}

#[derive(Debug, Clone)]
pub struct AddMarketplaceResult {
    pub name: String,
    pub already_materialized: bool,
    pub resolved_source: MarketplaceSource,
}

#[derive(Debug, Clone)]
pub enum PluginMarketplaceAddExecuteResult {
    Ok {
        plan: PluginMarketplaceAddPlan,
        name: String,
        already_materialized: bool,
        resolved_source: MarketplaceSource,
    },
    Err {
        error: PluginMarketplaceAddPlanError,
    },
}

static PLAN_STORE: Lazy<Mutex<HashMap<String, PluginMarketplaceAddPlan>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn prune_expired_plans() {
    let now = now_ms();
    let mut store = PLAN_STORE.lock().unwrap();
    store.retain(|_, plan| now - plan.created_at <= PLUGIN_MARKETPLACE_ADD_TOKEN_TTL_MS);
}

fn create_token() -> String {
    let store = PLAN_STORE.lock().unwrap();
    loop {
        let token = hex::encode(rand::random::<[u8; 4]>());
        if !store.contains_key(&token) {
            return token;
        }
    }
}

/// Create a marketplace add plan from user input.
pub async fn get_plugin_marketplace_add_plan(
    input: Option<&str>,
    parse_marketplace_input: impl std::future::Future<
        Output = Result<Option<MarketplaceSource>, String>,
    >,
    get_marketplace_source_display: impl Fn(&MarketplaceSource) -> String,
) -> PluginMarketplaceAddPlanResult {
    prune_expired_plans();

    let trimmed = match input.map(|s| s.trim()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return PluginMarketplaceAddPlanResult::Err {
                error: PluginMarketplaceAddPlanError::MissingSource,
            };
        }
    };

    let parsed = parse_marketplace_input.await;
    let source = match parsed {
        Ok(Some(s)) => s,
        Ok(None) => {
            return PluginMarketplaceAddPlanResult::Err {
                error: PluginMarketplaceAddPlanError::InvalidSource {
                    message:
                        "Invalid marketplace source format. Try: owner/repo, https://..., or ./path"
                            .to_string(),
                },
            };
        }
        Err(msg) => {
            return PluginMarketplaceAddPlanResult::Err {
                error: PluginMarketplaceAddPlanError::InvalidSource { message: msg },
            };
        }
    };

    let token = create_token();
    let plan = PluginMarketplaceAddPlan {
        token: token.clone(),
        created_at: now_ms(),
        input: trimmed,
        source_display: get_marketplace_source_display(&source),
        source,
    };

    PLAN_STORE.lock().unwrap().insert(token, plan.clone());
    PluginMarketplaceAddPlanResult::Ok { plan }
}

/// Execute a marketplace add plan by token.
pub async fn execute_plugin_marketplace_add_plan(
    token: &str,
    add_marketplace_source: impl FnOnce(
        &PluginMarketplaceAddPlan,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<AddMarketplaceResult, anyhow::Error>> + Send>,
    >,
    save_marketplace_to_settings: impl Fn(&str, &MarketplaceSource),
    clear_all_caches: impl Fn(),
) -> PluginMarketplaceAddExecuteResult {
    prune_expired_plans();

    let plan = {
        let mut store = PLAN_STORE.lock().unwrap();
        store.remove(token)
    };

    let plan = match plan {
        Some(p) => p,
        None => {
            return PluginMarketplaceAddExecuteResult::Err {
                error: PluginMarketplaceAddPlanError::UnknownToken {
                    token: token.to_string(),
                },
            };
        }
    };

    if now_ms() - plan.created_at > PLUGIN_MARKETPLACE_ADD_TOKEN_TTL_MS {
        return PluginMarketplaceAddExecuteResult::Err {
            error: PluginMarketplaceAddPlanError::ExpiredToken {
                token: token.to_string(),
            },
        };
    }

    match add_marketplace_source(&plan).await {
        Ok(result) => {
            save_marketplace_to_settings(&result.name, &result.resolved_source);
            clear_all_caches();
            PluginMarketplaceAddExecuteResult::Ok {
                plan,
                name: result.name,
                already_materialized: result.already_materialized,
                resolved_source: result.resolved_source,
            }
        }
        Err(error) => PluginMarketplaceAddExecuteResult::Err {
            error: PluginMarketplaceAddPlanError::AddFailed {
                message: error.to_string(),
            },
        },
    }
}

/// Test-only reset.
pub fn reset_plugin_marketplace_add_plan_store_for_testing() {
    PLAN_STORE.lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    static TEST_PLAN_STORE_MUTEX: Lazy<tokio::sync::Mutex<()>> =
        Lazy::new(|| tokio::sync::Mutex::new(()));

    fn github_source() -> MarketplaceSource {
        MarketplaceSource::GitHub {
            repo: "owner/repo".to_string(),
            git_ref: Some("main".to_string()),
            path: None,
            sparse_paths: None,
        }
    }

    #[tokio::test]
    async fn marketplace_add_plan_confirms_once_and_clears_caches() {
        let _guard = TEST_PLAN_STORE_MUTEX.lock().await;
        reset_plugin_marketplace_add_plan_store_for_testing();
        let source = github_source();
        let plan = match get_plugin_marketplace_add_plan(
            Some("owner/repo#main"),
            async { Ok(Some(github_source())) },
            |source| match source {
                MarketplaceSource::GitHub { repo, .. } => repo.clone(),
                other => format!("{other:?}"),
            },
        )
        .await
        {
            PluginMarketplaceAddPlanResult::Ok { plan } => plan,
            other => panic!("expected add plan, got {other:?}"),
        };

        assert!(!plan.token.is_empty());
        assert_eq!(plan.input, "owner/repo#main");
        assert_eq!(plan.source, source);
        assert_eq!(plan.source_display, "owner/repo");

        let saved = Arc::new(Mutex::new(Vec::<(String, MarketplaceSource)>::new()));
        let saved_for_closure = Arc::clone(&saved);
        let cleared = Arc::new(Mutex::new(0usize));
        let cleared_for_closure = Arc::clone(&cleared);

        let result = execute_plugin_marketplace_add_plan(
            &plan.token,
            |_plan| {
                Box::pin(async {
                    Ok(AddMarketplaceResult {
                        name: "owner-repo".to_string(),
                        already_materialized: false,
                        resolved_source: github_source(),
                    })
                })
            },
            move |name, source| {
                saved_for_closure
                    .lock()
                    .unwrap()
                    .push((name.to_string(), source.clone()));
            },
            move || {
                *cleared_for_closure.lock().unwrap() += 1;
            },
        )
        .await;

        match result {
            PluginMarketplaceAddExecuteResult::Ok {
                name,
                already_materialized,
                resolved_source,
                ..
            } => {
                assert_eq!(name, "owner-repo");
                assert!(!already_materialized);
                assert_eq!(resolved_source, source);
            }
            other => panic!("expected execute ok, got {other:?}"),
        }
        assert_eq!(
            saved.lock().unwrap().as_slice(),
            &[("owner-repo".to_string(), github_source())]
        );
        assert_eq!(*cleared.lock().unwrap(), 1);

        let second = execute_plugin_marketplace_add_plan(
            &plan.token,
            |_plan| {
                Box::pin(async {
                    Ok(AddMarketplaceResult {
                        name: "owner-repo".to_string(),
                        already_materialized: false,
                        resolved_source: github_source(),
                    })
                })
            },
            |_name, _source| {},
            || {},
        )
        .await;
        assert!(matches!(
            second,
            PluginMarketplaceAddExecuteResult::Err {
                error: PluginMarketplaceAddPlanError::UnknownToken { .. }
            }
        ));
    }

    #[tokio::test]
    async fn marketplace_add_plan_rejects_missing_and_invalid_sources() {
        let _guard = TEST_PLAN_STORE_MUTEX.lock().await;
        reset_plugin_marketplace_add_plan_store_for_testing();
        let missing =
            get_plugin_marketplace_add_plan(None, async { Ok(Some(github_source())) }, |_| {
                "unused".to_string()
            })
            .await;
        assert!(matches!(
            missing,
            PluginMarketplaceAddPlanResult::Err {
                error: PluginMarketplaceAddPlanError::MissingSource
            }
        ));

        let invalid = get_plugin_marketplace_add_plan(Some("bad"), async { Ok(None) }, |_| {
            "unused".to_string()
        })
        .await;
        assert!(matches!(
            invalid,
            PluginMarketplaceAddPlanResult::Err {
                error: PluginMarketplaceAddPlanError::InvalidSource { .. }
            }
        ));

        let parser_error = get_plugin_marketplace_add_plan(
            Some("./missing"),
            async { Err("Path does not exist".to_string()) },
            |_| "unused".to_string(),
        )
        .await;
        assert!(matches!(
            parser_error,
            PluginMarketplaceAddPlanResult::Err {
                error: PluginMarketplaceAddPlanError::InvalidSource { .. }
            }
        ));
    }
}
