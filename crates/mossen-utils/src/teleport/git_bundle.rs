//! Git bundle creation + upload for CCR seed-bundle seeding — translated
//! from `utils/teleport/gitBundle.ts`.
//!
//! Bundles the repository (`--all` → `HEAD` → squashed-root fallback) and
//! uploads the resulting bundle file. The upload step is injected via the
//! [`UploadFn`] alias so this module stays decoupled from the Files API
//! transport details.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// Default maximum bundle size in bytes (100 MiB), matching the TS constant
/// `DEFAULT_BUNDLE_MAX_BYTES`.
pub const DEFAULT_BUNDLE_MAX_BYTES: u64 = 100 * 1024 * 1024;

/// Bundle scope tier — how aggressive the fallback chain went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleScope {
    /// `git bundle create --all` succeeded under the size cap.
    All,
    /// `--all` exceeded the cap; HEAD-only succeeded.
    Head,
    /// HEAD also exceeded; bundled a squashed parentless commit.
    Squashed,
}

/// Reason for a bundle failure (forwarded to analytics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleFailReason {
    GitError,
    TooLarge,
    EmptyRepo,
}

/// Outcome of [`create_and_upload_git_bundle`].
#[derive(Debug, Clone)]
pub enum BundleUploadResult {
    Success {
        file_id: String,
        bundle_size_bytes: u64,
        scope: BundleScope,
        has_wip: bool,
    },
    Failure {
        error: String,
        fail_reason: Option<BundleFailReason>,
    },
}

/// Result of the bundle-creation step alone (before upload).
#[derive(Debug, Clone)]
pub enum BundleCreateResult {
    Ok { size: u64, scope: BundleScope },
    Err { error: String, fail_reason: BundleFailReason },
}

/// Successful upload report.
#[derive(Debug, Clone)]
pub struct UploadOk {
    pub file_id: String,
    pub size: u64,
}

/// Upload outcome — either success or a (non-fatal) error description.
#[derive(Debug, Clone)]
pub enum UploadResult {
    Ok(UploadOk),
    Err(String),
}

/// Boxed-future return for [`UploadFn`].
pub type UploadFuture<'a> = Pin<Box<dyn Future<Output = UploadResult> + Send + 'a>>;

/// Signature for the upload callback. Receives the path to the bundle file
/// and the desired remote relative path.
pub type UploadFn = Box<
    dyn for<'a> Fn(&'a Path, &'a str) -> UploadFuture<'a> + Send + Sync,
>;

/// Returns the user-facing "bundle too large" error message.
pub fn bundle_too_large_error(custom_backend_url: Option<&str>) -> String {
    match custom_backend_url {
        Some(url) => format!(
            "Repo is too large to bundle. Please connect GitHub from the hosted workspace at {}",
            url
        ),
        None => "Repo is too large to bundle. Please connect GitHub from the hosted workspace."
            .to_string(),
    }
}

/// Locate the `git` executable to invoke. Mirrors the TS `gitExe()` helper.
fn git_exe() -> String {
    std::env::var("GIT_EXE").unwrap_or_else(|_| "git".to_string())
}

async fn run_git(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(git_exe())
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .unwrap_or_else(|e| std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: Vec::new(),
            stderr: format!("failed to spawn git: {}", e).into_bytes(),
        })
}

async fn bundle_with_fallback(
    git_root: &Path,
    bundle_path: &Path,
    max_bytes: u64,
    has_stash: bool,
    bundle_too_large_msg: &str,
) -> BundleCreateResult {
    let bundle_path_str = bundle_path.to_string_lossy().into_owned();
    let stash_ref = "refs/seed/stash";
    let extra: Vec<&str> = if has_stash { vec![stash_ref] } else { vec![] };

    let mk_bundle = |base: &str| {
        let mut args: Vec<String> =
            vec!["bundle".into(), "create".into(), bundle_path_str.clone(), base.into()];
        for e in &extra {
            args.push((*e).to_string());
        }
        args
    };

    // --all
    let all_args = mk_bundle("--all");
    let all_refs: Vec<&str> = all_args.iter().map(|s| s.as_str()).collect();
    let all_result = run_git(git_root, &all_refs).await;
    if !all_result.status.success() {
        let stderr = String::from_utf8_lossy(&all_result.stderr);
        return BundleCreateResult::Err {
            error: format!(
                "git bundle create --all failed ({}): {}",
                all_result.status.code().unwrap_or(-1),
                truncate_str(&stderr, 200)
            ),
            fail_reason: BundleFailReason::GitError,
        };
    }
    let all_size = match tokio::fs::metadata(bundle_path).await {
        Ok(m) => m.len(),
        Err(e) => {
            return BundleCreateResult::Err {
                error: format!("Failed to stat bundle: {}", e),
                fail_reason: BundleFailReason::GitError,
            };
        }
    };
    if all_size <= max_bytes {
        return BundleCreateResult::Ok {
            size: all_size,
            scope: BundleScope::All,
        };
    }

    // HEAD
    let head_args = mk_bundle("HEAD");
    let head_refs: Vec<&str> = head_args.iter().map(|s| s.as_str()).collect();
    let head_result = run_git(git_root, &head_refs).await;
    if !head_result.status.success() {
        let stderr = String::from_utf8_lossy(&head_result.stderr);
        return BundleCreateResult::Err {
            error: format!(
                "git bundle create HEAD failed ({}): {}",
                head_result.status.code().unwrap_or(-1),
                truncate_str(&stderr, 200)
            ),
            fail_reason: BundleFailReason::GitError,
        };
    }
    let head_size = match tokio::fs::metadata(bundle_path).await {
        Ok(m) => m.len(),
        Err(e) => {
            return BundleCreateResult::Err {
                error: format!("Failed to stat bundle: {}", e),
                fail_reason: BundleFailReason::GitError,
            };
        }
    };
    if head_size <= max_bytes {
        return BundleCreateResult::Ok {
            size: head_size,
            scope: BundleScope::Head,
        };
    }

    // Squashed
    let tree_ref = if has_stash {
        "refs/seed/stash^{tree}"
    } else {
        "HEAD^{tree}"
    };
    let commit_tree = run_git(git_root, &["commit-tree", tree_ref, "-m", "seed"]).await;
    if !commit_tree.status.success() {
        let stderr = String::from_utf8_lossy(&commit_tree.stderr);
        return BundleCreateResult::Err {
            error: format!(
                "git commit-tree failed ({}): {}",
                commit_tree.status.code().unwrap_or(-1),
                truncate_str(&stderr, 200)
            ),
            fail_reason: BundleFailReason::GitError,
        };
    }
    let squashed_sha = String::from_utf8_lossy(&commit_tree.stdout).trim().to_string();
    let _ = run_git(git_root, &["update-ref", "refs/seed/root", &squashed_sha]).await;

    let squash_result = run_git(
        git_root,
        &["bundle", "create", &bundle_path_str, "refs/seed/root"],
    )
    .await;
    if !squash_result.status.success() {
        let stderr = String::from_utf8_lossy(&squash_result.stderr);
        return BundleCreateResult::Err {
            error: format!(
                "git bundle create refs/seed/root failed ({}): {}",
                squash_result.status.code().unwrap_or(-1),
                truncate_str(&stderr, 200)
            ),
            fail_reason: BundleFailReason::GitError,
        };
    }
    let squash_size = match tokio::fs::metadata(bundle_path).await {
        Ok(m) => m.len(),
        Err(e) => {
            return BundleCreateResult::Err {
                error: format!("Failed to stat bundle: {}", e),
                fail_reason: BundleFailReason::GitError,
            };
        }
    };
    if squash_size <= max_bytes {
        return BundleCreateResult::Ok {
            size: squash_size,
            scope: BundleScope::Squashed,
        };
    }

    BundleCreateResult::Err {
        error: bundle_too_large_msg.to_string(),
        fail_reason: BundleFailReason::TooLarge,
    }
}

fn truncate_str(s: &str, max: usize) -> &str {
    let end = s
        .char_indices()
        .nth(max)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    &s[..end]
}

/// Options accepted by [`create_and_upload_git_bundle`].
#[derive(Default)]
pub struct GitBundleOptions<'a> {
    /// Working directory (defaults to current dir).
    pub cwd: Option<&'a Path>,
    /// Bundle size cap; defaults to [`DEFAULT_BUNDLE_MAX_BYTES`].
    pub max_bytes: Option<u64>,
    /// Optional custom-backend URL used for the "too large" error message.
    pub custom_backend_url: Option<&'a str>,
    /// Optional override for the bundle file path (defaults to a tempfile).
    pub bundle_path: Option<PathBuf>,
}

/// Bundle the repo and upload it via the supplied callback.
///
/// Mirrors TS `createAndUploadGitBundle`. The TS version performs analytics
/// logging in the success/failure paths; in Rust analytics is the caller's
/// responsibility via the returned outcome.
pub async fn create_and_upload_git_bundle(
    upload: UploadFn,
    options: GitBundleOptions<'_>,
) -> BundleUploadResult {
    let workdir = match options.cwd {
        Some(p) => p.to_path_buf(),
        None => match std::env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                return BundleUploadResult::Failure {
                    error: format!("Unable to determine cwd: {}", e),
                    fail_reason: None,
                };
            }
        },
    };

    let git_root = match find_git_root(&workdir).await {
        Some(p) => p,
        None => {
            return BundleUploadResult::Failure {
                error: "Not in a git repository".to_string(),
                fail_reason: None,
            };
        }
    };

    let too_large_msg = bundle_too_large_error(options.custom_backend_url);

    // Sweep stale seed refs left over by a previous crashed run.
    for r in ["refs/seed/stash", "refs/seed/root"] {
        let _ = run_git(&git_root, &["update-ref", "-d", r]).await;
    }

    // Empty-repo check
    let ref_check = run_git(&git_root, &["for-each-ref", "--count=1", "refs/"]).await;
    if ref_check.status.success()
        && String::from_utf8_lossy(&ref_check.stdout).trim().is_empty()
    {
        return BundleUploadResult::Failure {
            error: "Repository has no commits yet".to_string(),
            fail_reason: Some(BundleFailReason::EmptyRepo),
        };
    }

    // WIP via stash create
    let stash_result = run_git(&git_root, &["stash", "create"]).await;
    let wip_sha = if stash_result.status.success() {
        String::from_utf8_lossy(&stash_result.stdout).trim().to_string()
    } else {
        String::new()
    };
    let has_wip = !wip_sha.is_empty();
    if has_wip {
        let _ = run_git(&git_root, &["update-ref", "refs/seed/stash", &wip_sha]).await;
    }

    let bundle_path = options
        .bundle_path
        .unwrap_or_else(|| std::env::temp_dir().join("ccr-seed.bundle"));

    let max_bytes = options.max_bytes.unwrap_or(DEFAULT_BUNDLE_MAX_BYTES);

    let outcome = async {
        let bundle = bundle_with_fallback(&git_root, &bundle_path, max_bytes, has_wip, &too_large_msg)
            .await;

        let (size, scope) = match bundle {
            BundleCreateResult::Ok { size, scope } => (size, scope),
            BundleCreateResult::Err { error, fail_reason } => {
                return BundleUploadResult::Failure {
                    error,
                    fail_reason: Some(fail_reason),
                };
            }
        };

        let upload_outcome = upload(&bundle_path, "_source_seed.bundle").await;
        match upload_outcome {
            UploadResult::Ok(ok) => BundleUploadResult::Success {
                file_id: ok.file_id,
                bundle_size_bytes: ok.size.max(size),
                scope,
                has_wip,
            },
            UploadResult::Err(e) => BundleUploadResult::Failure {
                error: e,
                fail_reason: None,
            },
        }
    }
    .await;

    // Cleanup: bundle file and seed refs.
    let _ = tokio::fs::remove_file(&bundle_path).await;
    for r in ["refs/seed/stash", "refs/seed/root"] {
        let _ = run_git(&git_root, &["update-ref", "-d", r]).await;
    }

    outcome
}

async fn find_git_root(start: &Path) -> Option<PathBuf> {
    let output = Command::new(git_exe())
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(start)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

/// Human-readable error message produced when a git bundle exceeds the
/// runtime size cap (mirror of TS `getBundleTooLargeError`). When a custom
/// backend is configured, callers should supply its remote-web URL; the TS
/// version reads it via `getHostedPlatformUrls()`, but we keep the Rust
/// port free of that dependency by taking it as an argument.
pub fn get_bundle_too_large_error(custom_remote_web_url: Option<&str>) -> String {
    if let Some(url) = custom_remote_web_url {
        return format!(
            "Repo is too large to bundle. Please connect GitHub from the hosted workspace at {}",
            url
        );
    }
    "Repo is too large to bundle. Please connect GitHub from the hosted workspace.".to_string()
}
