//! Semver comparison utilities.
//!
//! This module provides semver comparison functions.
//! Uses simple string-based comparison for common semver patterns.

/// Compare two version strings.
/// Returns -1 if a < b, 0 if a == b, 1 if a > b.
pub fn order(a: &str, b: &str) -> i32 {
    let va = parse_version(a);
    let vb = parse_version(b);
    let num = compare_versions(&va, &vb);
    if num != 0 {
        return num;
    }
    // Numeric parts equal; prerelease versions sort before release.
    let a_has_prerelease = a.contains('-');
    let b_has_prerelease = b.contains('-');
    if a_has_prerelease && !b_has_prerelease {
        return -1;
    }
    if !a_has_prerelease && b_has_prerelease {
        return 1;
    }
    0
}

/// Check if version a > version b
pub fn gt(a: &str, b: &str) -> bool {
    order(a, b) == 1
}

/// Check if version a >= version b
pub fn gte(a: &str, b: &str) -> bool {
    order(a, b) >= 0
}

/// Check if version a < version b
pub fn lt(a: &str, b: &str) -> bool {
    order(a, b) == -1
}

/// Check if version a <= version b
pub fn lte(a: &str, b: &str) -> bool {
    order(a, b) <= 0
}

/// Check if version satisfies the range.
pub fn satisfies(version: &str, range: &str) -> bool {
    let version_parts = parse_version(version);
    let range_str = range;

    // Handle common range patterns
    if range_str.starts_with("^") {
        let min_str = &range_str[1..];
        let min = parse_version(min_str);
        // ^1.0.0 means >=1.0.0 and <2.0.0
        if min.is_empty() {
            return false;
        }
        let major = min[0];
        return version_parts[0] == major && compare_versions(&version_parts, &min) >= 0;
    }
    if range_str.starts_with('~') {
        let min_str = &range_str[1..];
        let min = parse_version(min_str);
        // ~1.0.0 means >=1.0.0 and <1.1.0 (tilde-patch — npm standard).
        // Version must have same major + minor, and be >= min.
        if min.len() < 2 {
            return false;
        }
        let (major, minor) = (min[0], min[1]);
        return version_parts[0] == major
            && version_parts[1] == minor
            && compare_versions(&version_parts, &min) >= 0;
    }
    if range_str.starts_with(">=") {
        let min_str = &range_str[2..];
        let min = parse_version(min_str);
        return compare_versions(&version_parts, &min) >= 0;
    }
    if range_str.starts_with('>') && !range_str.starts_with(">=") {
        let min_str = &range_str[1..];
        let min = parse_version(min_str);
        return compare_versions(&version_parts, &min) > 0;
    }
    if range_str.starts_with("<=") {
        let max_str = &range_str[2..];
        let max = parse_version(max_str);
        return compare_versions(&version_parts, &max) <= 0;
    }
    if range_str.starts_with('<') && !range_str.starts_with("<=") {
        let max_str = &range_str[1..];
        let max = parse_version(max_str);
        return compare_versions(&version_parts, &max) < 0;
    }

    // Exact match
    version == range_str
}

/// Compare two version parts vectors.
/// Returns -1 if a < b, 0 if a == b, 1 if a > b.
fn compare_versions(a: &[u32], b: &[u32]) -> i32 {
    let max_len = a.len().max(b.len());

    for i in 0..max_len {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);

        if av < bv {
            return -1;
        }
        if av > bv {
            return 1;
        }
    }

    0
}

/// Parse a version string into parts
fn parse_version(s: &str) -> Vec<u32> {
    // Remove 'v' prefix if present
    let s = s.trim_start_matches('v');

    // Split by dots and parse as integers
    s.split('.')
        .map(|part| {
            // Extract leading numeric portion
            let numeric: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            numeric.parse::<u32>().unwrap_or(0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order() {
        assert_eq!(order("1.0.0", "2.0.0"), -1);
        assert_eq!(order("2.0.0", "1.0.0"), 1);
        assert_eq!(order("1.0.0", "1.0.0"), 0);
    }

    #[test]
    fn test_gt() {
        assert!(gt("2.0.0", "1.0.0"));
        assert!(!gt("1.0.0", "2.0.0"));
        assert!(!gt("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_gte() {
        assert!(gte("2.0.0", "1.0.0"));
        assert!(gte("1.0.0", "1.0.0"));
        assert!(!gte("1.0.0", "2.0.0"));
    }

    #[test]
    fn test_lt() {
        assert!(lt("1.0.0", "2.0.0"));
        assert!(!lt("2.0.0", "1.0.0"));
        assert!(!lt("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_lte() {
        assert!(lte("1.0.0", "2.0.0"));
        assert!(lte("1.0.0", "1.0.0"));
        assert!(!lte("2.0.0", "1.0.0"));
    }

    #[test]
    fn test_satisfies_caret() {
        assert!(satisfies("1.2.3", "^1.0.0"));
        assert!(satisfies("1.0.0", "^1.0.0"));
        assert!(satisfies("1.9.9", "^1.0.0"));
        assert!(!satisfies("2.0.0", "^1.0.0"));
        assert!(!satisfies("0.9.9", "^1.0.0"));
    }

    #[test]
    fn test_satisfies_tilde() {
        // Tilde-patch: ~1.0.0 means >=1.0.0 and <1.1.0.
        // Note: 1.0.3 (not 1.2.3) is within range because only patch
        // version changes are allowed under tilde-patch.
        assert!(satisfies("1.0.3", "~1.0.0"));
        assert!(satisfies("1.0.0", "~1.0.0"));
        assert!(!satisfies("1.1.0", "~1.0.0"));
        assert!(!satisfies("2.0.0", "~1.0.0"));
    }

    #[test]
    fn test_satisfies_gte() {
        assert!(satisfies("2.0.0", ">=1.0.0"));
        assert!(satisfies("1.0.0", ">=1.0.0"));
        assert!(!satisfies("0.9.9", ">=1.0.0"));
    }

    #[test]
    fn test_with_v_prefix() {
        assert!(gt("v2.0.0", "v1.0.0"));
        assert!(lt("v1.0.0", "v2.0.0"));
        assert!(satisfies("v1.2.3", "^1.0.0"));
    }

    #[test]
    fn test_prerelease() {
        // Basic test without prerelease handling
        assert!(gt("1.0.1", "1.0.0"));
        assert!(lt("1.0.0-alpha", "1.0.0"));
    }
}
