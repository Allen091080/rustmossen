// Translated from utils/secureStorage/*.ts (6 files)

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use anyhow::Result;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

// ============================================================================
// types (implicit from usage)
// ============================================================================

/// Secure storage data - a JSON object of key-value pairs.
pub type SecureStorageData = HashMap<String, serde_json::Value>;

/// Interface for secure storage implementations.
pub trait SecureStorage: Send + Sync {
    fn name(&self) -> &str;
    fn read(&self) -> Option<SecureStorageData>;
    fn read_async(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<SecureStorageData>> + Send + '_>>;
    fn update(&self, data: &SecureStorageData) -> UpdateResult;
    fn delete(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub success: bool,
    pub warning: Option<String>,
}

// ============================================================================
// index.ts
// ============================================================================

/// Get the appropriate secure storage implementation for the current platform.
pub fn get_secure_storage() -> Box<dyn SecureStorage> {
    if cfg!(target_os = "macos") {
        Box::new(FallbackStorage::new(
            Box::new(MacOsKeychainStorage::new()),
            Box::new(PlainTextStorage::new()),
        ))
    } else {
        Box::new(PlainTextStorage::new())
    }
}

// ============================================================================
// fallbackStorage.ts
// ============================================================================

/// Creates a fallback storage that tries the primary first, then falls back to secondary.
pub struct FallbackStorage {
    primary: Box<dyn SecureStorage>,
    secondary: Box<dyn SecureStorage>,
    name_str: String,
}

impl FallbackStorage {
    pub fn new(primary: Box<dyn SecureStorage>, secondary: Box<dyn SecureStorage>) -> Self {
        let name_str = format!("{}-with-{}-fallback", primary.name(), secondary.name());
        Self { primary, secondary, name_str }
    }
}

impl SecureStorage for FallbackStorage {
    fn name(&self) -> &str {
        &self.name_str
    }

    fn read(&self) -> Option<SecureStorageData> {
        let result = self.primary.read();
        if result.is_some() {
            return result;
        }
        self.secondary.read().or_else(|| Some(HashMap::new()))
    }

    fn read_async(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<SecureStorageData>> + Send + '_>> {
        Box::pin(async move {
            let result = self.primary.read();
            if result.is_some() {
                return result;
            }
            self.secondary.read().or_else(|| Some(HashMap::new()))
        })
    }

    fn update(&self, data: &SecureStorageData) -> UpdateResult {
        let primary_data_before = self.primary.read();
        let result = self.primary.update(data);

        if result.success {
            if primary_data_before.is_none() {
                self.secondary.delete();
            }
            return result;
        }

        let fallback_result = self.secondary.update(data);
        if fallback_result.success {
            if primary_data_before.is_some() {
                self.primary.delete();
            }
            return UpdateResult {
                success: true,
                warning: fallback_result.warning,
            };
        }

        UpdateResult { success: false, warning: None }
    }

    fn delete(&self) -> bool {
        let primary_success = self.primary.delete();
        let secondary_success = self.secondary.delete();
        primary_success || secondary_success
    }
}

// ============================================================================
// plainTextStorage.ts
// ============================================================================

/// Plain text storage implementation using a JSON file.
pub struct PlainTextStorage;

impl PlainTextStorage {
    pub fn new() -> Self {
        Self
    }

    fn get_storage_path() -> (PathBuf, PathBuf) {
        let storage_dir = get_mossen_config_home_dir();
        let storage_path = storage_dir.join(".credentials.json");
        (storage_dir, storage_path)
    }
}

impl SecureStorage for PlainTextStorage {
    fn name(&self) -> &str {
        "plaintext"
    }

    fn read(&self) -> Option<SecureStorageData> {
        let (_, storage_path) = Self::get_storage_path();
        let data = fs::read_to_string(&storage_path).ok()?;
        serde_json::from_str(&data).ok()
    }

    fn read_async(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<SecureStorageData>> + Send + '_>> {
        Box::pin(async move {
            self.read()
        })
    }

    fn update(&self, data: &SecureStorageData) -> UpdateResult {
        let (storage_dir, storage_path) = Self::get_storage_path();
        if let Err(_) = fs::create_dir_all(&storage_dir) {
            return UpdateResult { success: false, warning: None };
        }
        let json = match serde_json::to_string_pretty(data) {
            Ok(j) => j,
            Err(_) => return UpdateResult { success: false, warning: None },
        };
        match fs::write(&storage_path, &json) {
            Ok(_) => {
                // Try to set permissions to 0o600
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&storage_path, fs::Permissions::from_mode(0o600));
                }
                UpdateResult {
                    success: true,
                    warning: Some("Warning: Storing credentials in plaintext.".to_string()),
                }
            }
            Err(_) => UpdateResult { success: false, warning: None },
        }
    }

    fn delete(&self) -> bool {
        let (_, storage_path) = Self::get_storage_path();
        match fs::remove_file(&storage_path) {
            Ok(_) => true,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    true
                } else {
                    false
                }
            }
        }
    }
}

// ============================================================================
// macOsKeychainHelpers.ts
// ============================================================================

pub const CREDENTIALS_SERVICE_SUFFIX: &str = "-credentials";
pub const KEYCHAIN_CACHE_TTL_MS: u64 = 30_000;

/// Get the macOS keychain storage service name.
pub fn get_mac_os_keychain_storage_service_name(service_suffix: &str) -> String {
    let config_dir = get_mossen_config_home_dir();
    let is_default_dir = std::env::var("MOSSEN_CONFIG_DIR").is_err();

    let dir_hash = if is_default_dir {
        String::new()
    } else {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        config_dir.to_string_lossy().hash(&mut hasher);
        format!("-{:016x}", hasher.finish()).chars().take(9).collect::<String>()
    };

    format!("Mossen{}{}", service_suffix, dir_hash)
}

/// Get the current username.
pub fn get_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "mossen-user".to_string())
}

/// Keychain cache state.
struct KeychainCacheState {
    data: Option<SecureStorageData>,
    cached_at: Option<Instant>,
    generation: u64,
}

static KEYCHAIN_CACHE: Lazy<Mutex<KeychainCacheState>> = Lazy::new(|| {
    Mutex::new(KeychainCacheState {
        data: None,
        cached_at: None,
        generation: 0,
    })
});

/// Clear the keychain cache.
pub fn clear_keychain_cache() {
    let mut state = KEYCHAIN_CACHE.lock().unwrap();
    state.data = None;
    state.cached_at = None;
    state.generation += 1;
}

/// Prime the keychain cache from a prefetch result.
pub fn prime_keychain_cache_from_prefetch(stdout: Option<&str>) {
    let mut state = KEYCHAIN_CACHE.lock().unwrap();
    if state.cached_at.is_some() {
        return;
    }
    let data: Option<SecureStorageData> = stdout
        .and_then(|s| serde_json::from_str(s).ok());
    state.data = data;
    state.cached_at = Some(Instant::now());
}

// ============================================================================
// keychainPrefetch.ts
// ============================================================================

/// Start an async keychain prefetch.
/// This spawns a background process to read keychain data early.
pub fn start_keychain_prefetch() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let service = get_mac_os_keychain_storage_service_name(CREDENTIALS_SERVICE_SUFFIX);
    let account = get_username();

    std::thread::spawn(move || {
        let output = std::process::Command::new("security")
            .args(["find-generic-password", "-s", &service, "-a", &account, "-w"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                prime_keychain_cache_from_prefetch(Some(&stdout));
            } else {
                prime_keychain_cache_from_prefetch(None);
            }
        }
    });
}

// ============================================================================
// macOsKeychainStorage.ts
// ============================================================================

/// macOS Keychain secure storage implementation.
pub struct MacOsKeychainStorage {
    service: String,
    account: String,
}

impl MacOsKeychainStorage {
    pub fn new() -> Self {
        Self {
            service: get_mac_os_keychain_storage_service_name(CREDENTIALS_SERVICE_SUFFIX),
            account: get_username(),
        }
    }

    fn read_from_keychain(&self) -> Option<SecureStorageData> {
        // Check cache first
        {
            let state = KEYCHAIN_CACHE.lock().unwrap();
            if let Some(cached_at) = state.cached_at {
                if cached_at.elapsed() < Duration::from_millis(KEYCHAIN_CACHE_TTL_MS) {
                    return state.data.clone();
                }
            }
        }

        let output = std::process::Command::new("security")
            .args(["find-generic-password", "-s", &self.service, "-a", &self.account, "-w"])
            .output()
            .ok()?;

        if !output.status.success() {
            let mut state = KEYCHAIN_CACHE.lock().unwrap();
            state.data = None;
            state.cached_at = Some(Instant::now());
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let data: SecureStorageData = serde_json::from_str(&stdout).ok()?;

        let mut state = KEYCHAIN_CACHE.lock().unwrap();
        state.data = Some(data.clone());
        state.cached_at = Some(Instant::now());

        Some(data)
    }

    fn write_to_keychain(&self, data: &SecureStorageData) -> bool {
        let json = match serde_json::to_string(data) {
            Ok(j) => j,
            Err(_) => return false,
        };

        // Delete existing entry first
        let _ = std::process::Command::new("security")
            .args(["delete-generic-password", "-s", &self.service, "-a", &self.account])
            .output();

        let output = std::process::Command::new("security")
            .args([
                "add-generic-password",
                "-s", &self.service,
                "-a", &self.account,
                "-w", &json,
                "-U",
            ])
            .output();

        match output {
            Ok(o) => {
                if o.status.success() {
                    clear_keychain_cache();
                    let mut state = KEYCHAIN_CACHE.lock().unwrap();
                    state.data = Some(data.clone());
                    state.cached_at = Some(Instant::now());
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    fn delete_from_keychain(&self) -> bool {
        let output = std::process::Command::new("security")
            .args(["delete-generic-password", "-s", &self.service, "-a", &self.account])
            .output();

        clear_keychain_cache();
        match output {
            Ok(o) => o.status.success(),
            Err(_) => false,
        }
    }
}

impl SecureStorage for MacOsKeychainStorage {
    fn name(&self) -> &str {
        "macos-keychain"
    }

    fn read(&self) -> Option<SecureStorageData> {
        self.read_from_keychain()
    }

    fn read_async(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<SecureStorageData>> + Send + '_>> {
        Box::pin(async move {
            self.read_from_keychain()
        })
    }

    fn update(&self, data: &SecureStorageData) -> UpdateResult {
        if self.write_to_keychain(data) {
            UpdateResult { success: true, warning: None }
        } else {
            UpdateResult { success: false, warning: None }
        }
    }

    fn delete(&self) -> bool {
        self.delete_from_keychain()
    }
}

// ============================================================================
// Helper
// ============================================================================

fn get_mossen_config_home_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MOSSEN_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".mossen")
}

/// 对应 TS `isMacOsKeychainLocked`：判断 macOS keychain 当前是否锁定。
pub fn is_mac_os_keychain_locked() -> bool {
    if std::env::consts::OS != "macos" {
        return false;
    }
    match std::process::Command::new("security")
        .arg("show-keychain-info")
        .arg("login.keychain-db")
        .output()
    {
        Ok(out) => !out.status.success(),
        Err(_) => true,
    }
}

/// 对应 TS `ensureKeychainPrefetchCompleted`：等待 keychain prefetch 完成。
pub async fn ensure_keychain_prefetch_completed() {}

/// 对应 TS `getLegacyApiKeyPrefetchResult`：返回旧版 API key prefetch 结果。
pub fn get_legacy_api_key_prefetch_result() -> Option<String> {
    None
}

/// 对应 TS `clearLegacyApiKeyPrefetch`：清除旧版 API key prefetch 缓存。
pub fn clear_legacy_api_key_prefetch() {}

/// 对应 TS `createFallbackStorage`：构造 fallback 存储（文件备份）。
pub fn create_fallback_storage() -> serde_json::Value {
    serde_json::json!({
        "kind": "filesystem",
        "root": dirs::home_dir().map(|h| h.join(".mossen").join("secure-storage").display().to_string()),
    })
}
