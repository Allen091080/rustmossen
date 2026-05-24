//! # semver — 语义化版本比较工具
//!
//! 对应 TypeScript `utils/semver.ts`。
//! 使用自定义实现进行版本比较（避免外部 semver crate 依赖）。

/// 解析后的语义化版本。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SemVer {
    major: u64,
    minor: u64,
    patch: u64,
    pre: String,
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.major.cmp(&other.major) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        // Pre-release: empty means release (higher), non-empty means pre-release (lower)
        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => self.pre.cmp(&other.pre),
        }
    }
}

/// 解析版本字符串（宽松模式：忽略 build metadata 后的 +SHA）。
fn parse_loose(v: &str) -> Option<SemVer> {
    let cleaned = v.trim();
    let cleaned = cleaned.strip_prefix('v').unwrap_or(cleaned);
    // Remove build metadata (+...)
    let cleaned = cleaned.split('+').next().unwrap_or(cleaned);
    // Split pre-release
    let (version_part, pre) = if let Some(idx) = cleaned.find('-') {
        (&cleaned[..idx], cleaned[idx + 1..].to_string())
    } else {
        (cleaned, String::new())
    };
    let parts: Vec<&str> = version_part.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let major = parts[0].parse::<u64>().ok()?;
    let minor = parts
        .get(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let patch = parts
        .get(2)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    Some(SemVer {
        major,
        minor,
        patch,
        pre,
    })
}

/// 版本 a 是否大于版本 b。
pub fn gt(a: &str, b: &str) -> bool {
    match (parse_loose(a), parse_loose(b)) {
        (Some(va), Some(vb)) => va > vb,
        _ => false,
    }
}

/// 版本 a 是否大于等于版本 b。
pub fn gte(a: &str, b: &str) -> bool {
    match (parse_loose(a), parse_loose(b)) {
        (Some(va), Some(vb)) => va >= vb,
        _ => false,
    }
}

/// 版本 a 是否小于版本 b。
pub fn lt(a: &str, b: &str) -> bool {
    match (parse_loose(a), parse_loose(b)) {
        (Some(va), Some(vb)) => va < vb,
        _ => false,
    }
}

/// 版本 a 是否小于等于版本 b。
pub fn lte(a: &str, b: &str) -> bool {
    match (parse_loose(a), parse_loose(b)) {
        (Some(va), Some(vb)) => va <= vb,
        _ => false,
    }
}

/// 版本是否满足给定范围。
/// 支持简单范围：`>=1.0.0`, `<2.0.0`, `^1.2.3`, `~1.2.3`, `*`, 以及
/// 空格分隔的 AND 组合（如 `>=1.0.0 <2.0.0`）。
pub fn satisfies(version: &str, range: &str) -> bool {
    let ver = match parse_loose(version) {
        Some(v) => v,
        None => return false,
    };

    let trimmed = range.trim();
    if trimmed == "*" || trimmed.is_empty() {
        return true;
    }

    // Split by space for AND conditions
    let conditions: Vec<&str> = trimmed.split_whitespace().collect();
    for cond in conditions {
        if !satisfies_single(&ver, cond) {
            return false;
        }
    }
    true
}

/// Check a single range condition.
fn satisfies_single(ver: &SemVer, cond: &str) -> bool {
    if cond == "*" {
        return true;
    }
    if let Some(rest) = cond.strip_prefix(">=") {
        return match parse_loose(rest) {
            Some(bound) => *ver >= bound,
            None => false,
        };
    }
    if let Some(rest) = cond.strip_prefix("<=") {
        return match parse_loose(rest) {
            Some(bound) => *ver <= bound,
            None => false,
        };
    }
    if let Some(rest) = cond.strip_prefix('>') {
        return match parse_loose(rest) {
            Some(bound) => *ver > bound,
            None => false,
        };
    }
    if let Some(rest) = cond.strip_prefix('<') {
        return match parse_loose(rest) {
            Some(bound) => *ver < bound,
            None => false,
        };
    }
    if let Some(rest) = cond.strip_prefix('^') {
        // Caret: >=version, <next-major (or <next-minor if major==0)
        return match parse_loose(rest) {
            Some(bound) => {
                if *ver < bound {
                    return false;
                }
                if bound.major > 0 {
                    ver.major == bound.major
                } else if bound.minor > 0 {
                    ver.major == 0 && ver.minor == bound.minor
                } else {
                    ver.major == 0 && ver.minor == 0 && ver.patch == bound.patch
                }
            }
            None => false,
        };
    }
    if let Some(rest) = cond.strip_prefix('~') {
        // Tilde: >=version, <next-minor
        return match parse_loose(rest) {
            Some(bound) => {
                if *ver < bound {
                    return false;
                }
                ver.major == bound.major && ver.minor == bound.minor
            }
            None => false,
        };
    }
    if let Some(rest) = cond.strip_prefix('=') {
        return match parse_loose(rest) {
            Some(bound) => *ver == bound,
            None => false,
        };
    }
    // Exact match
    match parse_loose(cond) {
        Some(bound) => *ver == bound,
        None => false,
    }
}

/// 比较两个版本的顺序。
/// 返回 -1（a < b）、0（a == b）或 1（a > b）。
pub fn order(a: &str, b: &str) -> i8 {
    match (parse_loose(a), parse_loose(b)) {
        (Some(va), Some(vb)) => {
            if va < vb {
                -1
            } else if va > vb {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}
