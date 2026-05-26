//! Update Notification hook (useUpdateNotification.ts).
//! Shows notification when a new version is available.

#[derive(Debug, Clone)]
pub struct UpdateNotificationState {
    pub active: bool,
    pub initialized: bool,
}

impl UpdateNotificationState {
    pub fn new() -> Self {
        Self {
            active: false,
            initialized: false,
        }
    }
    pub fn initialize(&mut self) {
        self.initialized = true;
    }
    pub fn activate(&mut self) {
        self.active = true;
    }
    pub fn deactivate(&mut self) {
        self.active = false;
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
}
impl Default for UpdateNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the `major.minor.patch` slice from a possibly-extended semver
/// string (e.g. `1.2.3-beta.4+abc → "1.2.3"`).
///
/// TS source: `getSemverPart(version)`.
pub fn get_semver_part(version: &str) -> String {
    // Strip prerelease/build segments.
    let core = version.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    let pick = |idx: usize| -> u64 {
        parts
            .get(idx)
            .and_then(|p| {
                // Skip any non-digit suffix.
                let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse::<u64>().ok()
            })
            .unwrap_or(0)
    };
    format!("{}.{}.{}", pick(0), pick(1), pick(2))
}

/// True if the just-installed semver differs from the most recently
/// notified one — i.e. the user hasn't seen an update toast for this
/// version yet.
///
/// TS source: `shouldShowUpdateNotification(updatedVersion, lastNotifiedSemver)`.
pub fn should_show_update_notification(
    updated_version: &str,
    last_notified_semver: Option<&str>,
) -> bool {
    let updated = get_semver_part(updated_version);
    last_notified_semver != Some(updated.as_str())
}

#[cfg(test)]
mod update_notification_tests {
    use super::*;

    #[test]
    fn semver_part_strips_pre() {
        assert_eq!(get_semver_part("1.2.3-beta.4"), "1.2.3");
    }

    #[test]
    fn semver_part_strips_build() {
        assert_eq!(get_semver_part("1.2.3+sha.abc"), "1.2.3");
    }

    #[test]
    fn should_show_when_different() {
        assert!(should_show_update_notification("1.2.3", Some("1.2.2")));
        assert!(!should_show_update_notification("1.2.3", Some("1.2.3")));
        assert!(should_show_update_notification("1.2.3", None));
    }
}
