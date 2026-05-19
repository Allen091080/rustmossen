//! # find_executable — 查找可执行文件
//!
//! 对应 TypeScript `utils/findExecutable.ts`。
//! 通过搜索 PATH 查找可执行文件。

/// 查找可执行文件并返回命令和参数。
/// 类似于 `which` 命令。
/// 返回 { cmd, args } 以匹配 spawn-rx API 形状。
pub fn find_executable(exe: &str, args: &[String]) -> (String, Vec<String>) {
    // 通过 `which` crate 在 PATH 中查找可执行文件；
    // 如果未找到则返回原始名称（与 TS whichSync ?? exe 的行为一致）。
    let resolved = which::which(exe)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| exe.to_string());
    (resolved, args.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_executable_returns_original_when_not_found() {
        let (cmd, args) = find_executable("nonexistent_cmd", &["arg1".to_string()]);
        assert_eq!(cmd, "nonexistent_cmd");
        assert_eq!(args.len(), 1);
    }
}
