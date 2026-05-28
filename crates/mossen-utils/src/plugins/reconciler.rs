use std::collections::HashMap;
use std::path::Path;

use tracing::debug;

use super::schemas::{KnownMarketplacesFile, MarketplaceSource};

/// Diff result comparing declared intent vs materialized state.
#[derive(Debug, Clone)]
pub struct MarketplaceDiff {
    /// Declared in settings, absent from known_marketplaces.json
    pub missing: Vec<String>,
    /// Present in both, but settings source != JSON source
    pub source_changed: Vec<SourceChangedEntry>,
    /// Present in both, sources match
    pub up_to_date: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SourceChangedEntry {
    pub name: String,
    pub declared_source: MarketplaceSource,
    pub materialized_source: MarketplaceSource,
}

/// Info about a declared marketplace from settings (for reconciler).
#[derive(Debug, Clone)]
pub struct DeclaredMarketplace {
    pub source: MarketplaceSource,
    pub auto_update: Option<bool>,
    pub source_is_fallback: bool,
}

#[derive(Debug, Clone)]
pub struct ReconcileOptions<F1, F2>
where
    F1: Fn(&str, &MarketplaceSource) -> bool,
    F2: Fn(ReconcileProgressEvent),
{
    pub skip: Option<F1>,
    pub on_progress: Option<F2>,
}

#[derive(Debug, Clone)]
pub enum ReconcileProgressEvent {
    Installing {
        name: String,
        action: ReconcileAction,
        index: usize,
        total: usize,
    },
    Installed {
        name: String,
        already_materialized: bool,
    },
    Failed {
        name: String,
        error: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ReconcileAction {
    Install,
    Update,
}

#[derive(Debug, Clone)]
pub struct ReconcileResult {
    pub installed: Vec<String>,
    pub updated: Vec<String>,
    pub failed: Vec<ReconcileFailure>,
    pub up_to_date: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReconcileFailure {
    pub name: String,
    pub error: String,
}

/// Compare declared intent (settings) against materialized state (JSON).
///
/// Resolves relative directory/file paths before comparing.
pub fn diff_marketplaces(
    declared: &HashMap<String, DeclaredMarketplace>,
    materialized: &KnownMarketplacesFile,
    project_root: Option<&str>,
    find_canonical_git_root: impl Fn(&str) -> Option<String>,
) -> MarketplaceDiff {
    let mut missing = Vec::new();
    let mut source_changed = Vec::new();
    let mut up_to_date = Vec::new();

    for (name, intent) in declared {
        let normalized_intent =
            normalize_source(&intent.source, project_root, &find_canonical_git_root);

        match materialized.get(name) {
            None => {
                missing.push(name.clone());
            }
            Some(state) => {
                if intent.source_is_fallback {
                    // Fallback: presence suffices
                    up_to_date.push(name.clone());
                } else if normalized_intent != state.source {
                    source_changed.push(SourceChangedEntry {
                        name: name.clone(),
                        declared_source: normalized_intent,
                        materialized_source: state.source.clone(),
                    });
                } else {
                    up_to_date.push(name.clone());
                }
            }
        }
    }

    MarketplaceDiff {
        missing,
        source_changed,
        up_to_date,
    }
}

/// Make known_marketplaces.json consistent with declared intent.
/// Idempotent. Additive only (never deletes).
pub async fn reconcile_marketplaces(
    get_declared_marketplaces: impl Fn() -> HashMap<String, DeclaredMarketplace>,
    load_known_marketplaces_config: impl std::future::Future<
        Output = Result<KnownMarketplacesFile, anyhow::Error>,
    >,
    get_original_cwd: impl Fn() -> String,
    find_canonical_git_root: impl Fn(&str) -> Option<String>,
    add_marketplace_source: impl Fn(
        &MarketplaceSource,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        super::marketplace_add_plan::AddMarketplaceResult,
                        anyhow::Error,
                    >,
                > + Send,
        >,
    >,
    path_exists: impl Fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>>,
    skip: Option<&dyn Fn(&str, &MarketplaceSource) -> bool>,
    on_progress: Option<&dyn Fn(ReconcileProgressEvent)>,
) -> ReconcileResult {
    let declared = get_declared_marketplaces();
    if declared.is_empty() {
        return ReconcileResult {
            installed: vec![],
            updated: vec![],
            failed: vec![],
            up_to_date: vec![],
            skipped: vec![],
        };
    }

    let materialized = match load_known_marketplaces_config.await {
        Ok(m) => m,
        Err(e) => {
            debug!("Failed to load known marketplaces config: {}", e);
            HashMap::new()
        }
    };

    let project_root = get_original_cwd();
    let diff = diff_marketplaces(
        &declared,
        &materialized,
        Some(&project_root),
        &find_canonical_git_root,
    );

    struct WorkItem {
        name: String,
        source: MarketplaceSource,
        action: ReconcileAction,
    }

    let mut work: Vec<WorkItem> = Vec::new();
    for name in &diff.missing {
        work.push(WorkItem {
            name: name.clone(),
            source: normalize_source(
                &declared[name].source,
                Some(&project_root),
                &find_canonical_git_root,
            ),
            action: ReconcileAction::Install,
        });
    }
    for entry in &diff.source_changed {
        work.push(WorkItem {
            name: entry.name.clone(),
            source: entry.declared_source.clone(),
            action: ReconcileAction::Update,
        });
    }

    let mut skipped = Vec::new();
    let mut to_process = Vec::new();

    for item in work {
        if let Some(skip_fn) = skip {
            if skip_fn(&item.name, &item.source) {
                skipped.push(item.name);
                continue;
            }
        }
        // For sourceChanged local-path entries, skip if path doesn't exist
        if matches!(item.action, ReconcileAction::Update)
            && is_local_marketplace_source(&item.source)
        {
            let path = get_local_source_path(&item.source).unwrap_or_default();
            if !path_exists(&path).await {
                debug!(
                    "[reconcile] '{}' declared path does not exist; keeping materialized entry",
                    item.name
                );
                skipped.push(item.name);
                continue;
            }
        }
        to_process.push(item);
    }

    if to_process.is_empty() {
        return ReconcileResult {
            installed: vec![],
            updated: vec![],
            failed: vec![],
            up_to_date: diff.up_to_date,
            skipped,
        };
    }

    debug!(
        "[reconcile] {} marketplace(s): {}",
        to_process.len(),
        to_process
            .iter()
            .map(|w| format!("{}({:?})", w.name, w.action))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut installed = Vec::new();
    let mut updated = Vec::new();
    let mut failed = Vec::new();

    for (i, item) in to_process.iter().enumerate() {
        if let Some(progress_fn) = on_progress {
            progress_fn(ReconcileProgressEvent::Installing {
                name: item.name.clone(),
                action: item.action,
                index: i + 1,
                total: to_process.len(),
            });
        }

        match add_marketplace_source(&item.source).await {
            Ok(result) => {
                match item.action {
                    ReconcileAction::Install => installed.push(item.name.clone()),
                    ReconcileAction::Update => updated.push(item.name.clone()),
                }
                if let Some(progress_fn) = on_progress {
                    progress_fn(ReconcileProgressEvent::Installed {
                        name: item.name.clone(),
                        already_materialized: result.already_materialized,
                    });
                }
            }
            Err(e) => {
                let error = e.to_string();
                failed.push(ReconcileFailure {
                    name: item.name.clone(),
                    error: error.clone(),
                });
                if let Some(progress_fn) = on_progress {
                    progress_fn(ReconcileProgressEvent::Failed {
                        name: item.name.clone(),
                        error,
                    });
                }
            }
        }
    }

    ReconcileResult {
        installed,
        updated,
        failed,
        up_to_date: diff.up_to_date,
        skipped,
    }
}

/// Resolve relative directory/file paths for stable comparison.
fn normalize_source(
    source: &MarketplaceSource,
    project_root: Option<&str>,
    find_canonical_git_root: &dyn Fn(&str) -> Option<String>,
) -> MarketplaceSource {
    match source {
        MarketplaceSource::Directory { path } | MarketplaceSource::File { path }
            if !Path::new(path).is_absolute() =>
        {
            let base = project_root.unwrap_or(".");
            let canonical_root = find_canonical_git_root(base);
            let resolved_base = canonical_root.as_deref().unwrap_or(base);
            let resolved = Path::new(resolved_base).join(path);
            match source {
                MarketplaceSource::Directory { .. } => MarketplaceSource::Directory {
                    path: resolved.to_string_lossy().to_string(),
                },
                MarketplaceSource::File { .. } => MarketplaceSource::File {
                    path: resolved.to_string_lossy().to_string(),
                },
                _ => source.clone(),
            }
        }
        _ => source.clone(),
    }
}

fn is_local_marketplace_source(source: &MarketplaceSource) -> bool {
    matches!(
        source,
        MarketplaceSource::Directory { .. } | MarketplaceSource::File { .. }
    )
}

fn get_local_source_path(source: &MarketplaceSource) -> Option<String> {
    match source {
        MarketplaceSource::Directory { path } | MarketplaceSource::File { path } => {
            Some(path.clone())
        }
        _ => None,
    }
}
