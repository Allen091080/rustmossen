//! # config_snapshot — Hook 配置快照
//!
//! 对应 TS `utils/hooks/hooksConfigSnapshot.ts`。
//! 管理 Hook 配置的快照（捕获、更新、查询）。
//! 支持策略限制（managed-only、disable-all）。

use parking_lot::RwLock;

use super::settings::HooksSettings;

/// Hook 配置快照 — 保存启动时的 Hook 配置副本。
///
/// 对应 TS 中的 `initialHooksConfig` 全局状态。
pub struct HooksConfigSnapshot {
    /// 当前快照。
    config: RwLock<Option<HooksSettings>>,
    /// 是否仅允许受管 Hook。
    managed_only: RwLock<bool>,
    /// 是否禁用所有 Hook（包括受管）。
    all_disabled: RwLock<bool>,
}

impl HooksConfigSnapshot {
    /// 创建新的空快照。
    pub fn new() -> Self {
        Self {
            config: RwLock::new(None),
            managed_only: RwLock::new(false),
            all_disabled: RwLock::new(false),
        }
    }

    /// 捕获当前 Hook 配置快照。
    ///
    /// 对应 TS `captureHooksConfigSnapshot()`。
    pub fn capture(&self, settings: HooksSettings) {
        *self.config.write() = Some(settings);
    }

    /// 更新 Hook 配置快照。
    ///
    /// 对应 TS `updateHooksConfigSnapshot()`。
    pub fn update(&self, settings: HooksSettings) {
        *self.config.write() = Some(settings);
    }

    /// 获取当前配置快照。
    ///
    /// 对应 TS `getHooksConfigFromSnapshot()`。
    pub fn get(&self) -> Option<HooksSettings> {
        self.config.read().clone()
    }

    /// 重置快照（用于测试）。
    ///
    /// 对应 TS `resetHooksConfigSnapshot()`。
    pub fn reset(&self) {
        *self.config.write() = None;
    }

    /// 设置是否仅允许受管 Hook。
    pub fn set_managed_only(&self, managed_only: bool) {
        *self.managed_only.write() = managed_only;
    }

    /// 检查是否仅允许受管 Hook。
    ///
    /// 对应 TS `shouldAllowManagedHooksOnly()`。
    pub fn should_allow_managed_only(&self) -> bool {
        *self.managed_only.read()
    }

    /// 设置是否禁用所有 Hook。
    pub fn set_all_disabled(&self, disabled: bool) {
        *self.all_disabled.write() = disabled;
    }

    /// 检查是否禁用所有 Hook（包括受管）。
    ///
    /// 对应 TS `shouldDisableAllHooksIncludingManaged()`。
    pub fn should_disable_all(&self) -> bool {
        *self.all_disabled.read()
    }

    /// 检查快照是否已初始化。
    pub fn is_initialized(&self) -> bool {
        self.config.read().is_some()
    }
}

impl Default for HooksConfigSnapshot {
    fn default() -> Self {
        Self::new()
    }
}
