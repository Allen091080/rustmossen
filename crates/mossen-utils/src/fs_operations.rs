//! Filesystem operations abstraction.
//!
//! Provides a unified filesystem interface with both sync and async operations,
//! path resolution with symlink handling, and permission checking utilities.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Result of reading a file range.
#[derive(Debug, Clone)]
pub struct ReadFileRangeResult {
    pub content: String,
    pub bytes_read: usize,
    pub bytes_total: u64,
}

/// Result of safe path resolution.
#[derive(Debug, Clone)]
pub struct SafeResolveResult {
    pub resolved_path: PathBuf,
    pub is_symlink: bool,
    pub is_canonical: bool,
}

/// Safely resolves a file path, handling symlinks and errors gracefully.
pub fn safe_resolve_path(file_path: &Path) -> SafeResolveResult {
    let path_str = file_path.to_string_lossy();

    // Block UNC paths before any filesystem access
    if path_str.starts_with("//") || path_str.starts_with("\\\\") {
        return SafeResolveResult {
            resolved_path: file_path.to_path_buf(),
            is_symlink: false,
            is_canonical: false,
        };
    }

    // Check for special file types before calling canonicalize
    match std::fs::symlink_metadata(file_path) {
        Ok(meta) => {
            let file_type = meta.file_type();
            if file_type.is_symlink() {
                // It's a symlink, try to resolve
                match std::fs::canonicalize(file_path) {
                    Ok(resolved) => SafeResolveResult {
                        is_symlink: resolved != file_path,
                        resolved_path: resolved,
                        is_canonical: true,
                    },
                    Err(_) => SafeResolveResult {
                        resolved_path: file_path.to_path_buf(),
                        is_symlink: false,
                        is_canonical: false,
                    },
                }
            } else if cfg!(unix) {
                // Check for FIFOs, sockets, devices on unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::FileTypeExt;
                    if file_type.is_fifo()
                        || file_type.is_socket()
                        || file_type.is_char_device()
                        || file_type.is_block_device()
                    {
                        return SafeResolveResult {
                            resolved_path: file_path.to_path_buf(),
                            is_symlink: false,
                            is_canonical: false,
                        };
                    }
                }
                match std::fs::canonicalize(file_path) {
                    Ok(resolved) => SafeResolveResult {
                        is_symlink: resolved != file_path,
                        resolved_path: resolved,
                        is_canonical: true,
                    },
                    Err(_) => SafeResolveResult {
                        resolved_path: file_path.to_path_buf(),
                        is_symlink: false,
                        is_canonical: false,
                    },
                }
            } else {
                match std::fs::canonicalize(file_path) {
                    Ok(resolved) => SafeResolveResult {
                        is_symlink: resolved != file_path,
                        resolved_path: resolved,
                        is_canonical: true,
                    },
                    Err(_) => SafeResolveResult {
                        resolved_path: file_path.to_path_buf(),
                        is_symlink: false,
                        is_canonical: false,
                    },
                }
            }
        }
        Err(_) => SafeResolveResult {
            resolved_path: file_path.to_path_buf(),
            is_symlink: false,
            is_canonical: false,
        },
    }
}

/// Check if a file path is a duplicate and should be skipped.
pub fn is_duplicate_path(file_path: &Path, loaded_paths: &mut std::collections::HashSet<PathBuf>) -> bool {
    let result = safe_resolve_path(file_path);
    if loaded_paths.contains(&result.resolved_path) {
        return true;
    }
    loaded_paths.insert(result.resolved_path);
    false
}

/// Resolve the deepest existing ancestor of a path via canonicalize.
pub fn resolve_deepest_existing_ancestor(absolute_path: &Path) -> Option<PathBuf> {
    let mut dir = absolute_path.to_path_buf();
    let mut segments: Vec<String> = Vec::new();

    loop {
        let parent = match dir.parent() {
            Some(p) if p != dir => p.to_path_buf(),
            _ => break,
        };

        match std::fs::symlink_metadata(&dir) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    // Found a symlink
                    match std::fs::canonicalize(&dir) {
                        Ok(resolved) => {
                            return if segments.is_empty() {
                                Some(resolved)
                            } else {
                                let mut result = resolved;
                                for seg in segments.iter().rev() {
                                    result = result.join(seg);
                                }
                                Some(result)
                            };
                        }
                        Err(_) => {
                            // Dangling symlink
                            match std::fs::read_link(&dir) {
                                Ok(target) => {
                                    let abs_target = if target.is_absolute() {
                                        target
                                    } else {
                                        dir.parent()
                                            .unwrap_or(Path::new("/"))
                                            .join(&target)
                                    };
                                    return if segments.is_empty() {
                                        Some(abs_target)
                                    } else {
                                        let mut result = abs_target;
                                        for seg in segments.iter().rev() {
                                            result = result.join(seg);
                                        }
                                        Some(result)
                                    };
                                }
                                Err(_) => return None,
                            }
                        }
                    }
                }
                // Non-symlink existing component
                match std::fs::canonicalize(&dir) {
                    Ok(resolved) if resolved != dir => {
                        return if segments.is_empty() {
                            Some(resolved)
                        } else {
                            let mut result = resolved;
                            for seg in segments.iter().rev() {
                                result = result.join(seg);
                            }
                            Some(result)
                        };
                    }
                    _ => return None,
                }
            }
            Err(_) => {
                // Doesn't exist, walk up
                if let Some(name) = dir.file_name() {
                    segments.push(name.to_string_lossy().to_string());
                }
                dir = parent;
            }
        }
    }

    None
}

/// Gets all paths that should be checked for permissions.
pub fn get_paths_for_permission_check(input_path: &str) -> Vec<PathBuf> {
    let mut path = PathBuf::from(input_path);

    // Expand tilde
    if input_path == "~" {
        if let Some(home) = dirs::home_dir() {
            path = home;
        }
    } else if input_path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            path = home.join(&input_path[2..]);
        }
    }

    let mut path_set = std::collections::HashSet::new();
    path_set.insert(path.clone());

    // Block UNC paths
    let path_str = path.to_string_lossy();
    if path_str.starts_with("//") || path_str.starts_with("\\\\") {
        return path_set.into_iter().collect();
    }

    // Follow symlink chain
    let mut current_path = path.clone();
    let mut visited = std::collections::HashSet::new();
    let max_depth = 40;

    for _ in 0..max_depth {
        if visited.contains(&current_path) {
            break;
        }
        visited.insert(current_path.clone());

        if !current_path.exists() {
            // Try to resolve deepest existing ancestor
            if current_path == path {
                if let Some(resolved) = resolve_deepest_existing_ancestor(&path) {
                    path_set.insert(resolved);
                }
            }
            break;
        }

        match std::fs::symlink_metadata(&current_path) {
            Ok(meta) => {
                if !meta.file_type().is_symlink() {
                    break;
                }
                match std::fs::read_link(&current_path) {
                    Ok(target) => {
                        let absolute_target = if target.is_absolute() {
                            target
                        } else {
                            current_path
                                .parent()
                                .unwrap_or(Path::new("/"))
                                .join(&target)
                        };
                        path_set.insert(absolute_target.clone());
                        current_path = absolute_target;
                    }
                    Err(_) => break,
                }
            }
            Err(_) => break,
        }
    }

    // Also add final resolved path
    let resolve_result = safe_resolve_path(&path);
    if resolve_result.is_symlink && resolve_result.resolved_path != path {
        path_set.insert(resolve_result.resolved_path);
    }

    path_set.into_iter().collect()
}

/// Read up to `max_bytes` from a file starting at `offset`.
pub async fn read_file_range(
    path: &str,
    offset: u64,
    max_bytes: usize,
) -> io::Result<Option<ReadFileRangeResult>> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = tokio::fs::File::open(path).await?;
    let metadata = file.metadata().await?;
    let size = metadata.len();

    if size <= offset {
        return Ok(None);
    }

    let bytes_to_read = ((size - offset) as usize).min(max_bytes);
    let mut buffer = vec![0u8; bytes_to_read];

    file.seek(std::io::SeekFrom::Start(offset)).await?;
    let mut total_read = 0;
    while total_read < bytes_to_read {
        let n = file.read(&mut buffer[total_read..]).await?;
        if n == 0 {
            break;
        }
        total_read += n;
    }

    Ok(Some(ReadFileRangeResult {
        content: String::from_utf8_lossy(&buffer[..total_read]).to_string(),
        bytes_read: total_read,
        bytes_total: size,
    }))
}

/// Read the last `max_bytes` of a file.
pub async fn tail_file(path: &str, max_bytes: usize) -> io::Result<ReadFileRangeResult> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = tokio::fs::File::open(path).await?;
    let metadata = file.metadata().await?;
    let size = metadata.len();

    if size == 0 {
        return Ok(ReadFileRangeResult {
            content: String::new(),
            bytes_read: 0,
            bytes_total: 0,
        });
    }

    let offset = size.saturating_sub(max_bytes as u64);
    let bytes_to_read = (size - offset) as usize;
    let mut buffer = vec![0u8; bytes_to_read];

    file.seek(std::io::SeekFrom::Start(offset)).await?;
    let mut total_read = 0;
    while total_read < bytes_to_read {
        let n = file.read(&mut buffer[total_read..]).await?;
        if n == 0 {
            break;
        }
        total_read += n;
    }

    Ok(ReadFileRangeResult {
        content: String::from_utf8_lossy(&buffer[..total_read]).to_string(),
        bytes_read: total_read,
        bytes_total: size,
    })
}

/// Async generator-like iterator that yields lines from a file in reverse order.
pub async fn read_lines_reverse(path: &str) -> io::Result<Vec<String>> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    const CHUNK_SIZE: usize = 4096;

    let mut file = tokio::fs::File::open(path).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len() as usize;

    let mut position = file_size;
    let mut remainder = Vec::new();
    let mut lines = Vec::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];

    while position > 0 {
        let current_chunk_size = CHUNK_SIZE.min(position);
        position -= current_chunk_size;

        file.seek(std::io::SeekFrom::Start(position as u64)).await?;
        file.read_exact(&mut buffer[..current_chunk_size]).await?;

        let mut combined = Vec::with_capacity(current_chunk_size + remainder.len());
        combined.extend_from_slice(&buffer[..current_chunk_size]);
        combined.extend_from_slice(&remainder);

        let first_newline = combined.iter().position(|&b| b == b'\n');
        match first_newline {
            None => {
                remainder = combined;
            }
            Some(idx) => {
                remainder = combined[..idx].to_vec();
                let text = String::from_utf8_lossy(&combined[idx + 1..]).to_string();
                let chunk_lines: Vec<&str> = text.split('\n').collect();
                for line in chunk_lines.into_iter().rev() {
                    if !line.is_empty() {
                        lines.push(line.to_string());
                    }
                }
            }
        }
    }

    if !remainder.is_empty() {
        lines.push(String::from_utf8_lossy(&remainder).to_string());
    }

    Ok(lines)
}

/// Create directory recursively, ignoring EEXIST.
pub async fn mkdir_recursive(path: &str) -> io::Result<()> {
    match tokio::fs::create_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// Check if a path exists.
pub fn exists_sync(path: &str) -> bool {
    Path::new(path).exists()
}

/// Get errno code from an io::Error.
pub fn get_errno_code(error: &io::Error) -> Option<&'static str> {
    match error.kind() {
        io::ErrorKind::NotFound => Some("ENOENT"),
        io::ErrorKind::PermissionDenied => Some("EACCES"),
        io::ErrorKind::AlreadyExists => Some("EEXIST"),
        io::ErrorKind::WouldBlock => Some("EAGAIN"),
        io::ErrorKind::InvalidInput => Some("EINVAL"),
        io::ErrorKind::BrokenPipe => Some("EPIPE"),
        io::ErrorKind::ConnectionRefused => Some("ECONNREFUSED"),
        io::ErrorKind::ConnectionReset => Some("ECONNRESET"),
        io::ErrorKind::TimedOut => Some("ETIMEDOUT"),
        _ => None,
    }
}

/// Check if an error is ENOENT.
pub fn is_enoent(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound
}

// =============================================================================
// FsOperations 注入接口 — TS 使用一个 `FsOperations` 类型把 `fs.readFileSync`
// 等同义于结构体打包，便于测试时替换。Rust 端使用 `std::fs`/`tokio::fs`，因此
// 我们用 trait + 进程级注入点提供等价语义。
// =============================================================================

use std::fs;
use std::sync::{Mutex, OnceLock};

/// 对应 TS `FsOperations`：可注入的文件系统接口（trait 对象形态）。
pub trait FsOperationsTrait: Send + Sync {
    /// 读取整文件文本内容。
    fn read_file_sync(&self, path: &str) -> std::io::Result<String> {
        fs::read_to_string(path)
    }
    /// 写入文件文本内容。
    fn write_file_sync(&self, path: &str, contents: &str) -> std::io::Result<()> {
        fs::write(path, contents)
    }
    /// 判断路径是否存在。
    fn exists_sync(&self, path: &str) -> bool {
        Path::new(path).exists()
    }
    /// 返回当前工作目录。
    fn cwd(&self) -> String {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

/// 别名导出，符合 TS 同名引用。
pub type FsOperations = dyn FsOperationsTrait;

/// 默认 Node.js fs 等价实现（仅作为类型存在；可在测试中被替换）。
pub struct NodeFsOperationsImpl;
impl FsOperationsTrait for NodeFsOperationsImpl {}

/// 全局共享实例的名字，便于调用方使用 `&*NODE_FS_OPERATIONS` 引用。
pub static NODE_FS_OPERATIONS: NodeFsOperationsImpl = NodeFsOperationsImpl;

static FS_IMPL: OnceLock<Mutex<Box<dyn FsOperationsTrait>>> = OnceLock::new();
static FS_ORIGINAL_IMPL: OnceLock<Mutex<Box<dyn FsOperationsTrait>>> = OnceLock::new();

fn ensure_default() -> &'static Mutex<Box<dyn FsOperationsTrait>> {
    FS_IMPL.get_or_init(|| Mutex::new(Box::new(NodeFsOperationsImpl)))
}

/// 设置当前 `FsOperations`（对应 TS `setFsImplementation`）。
pub fn set_fs_implementation(impl_: Box<dyn FsOperationsTrait>) {
    let cell = ensure_default();
    *cell.lock().unwrap() = impl_;
}

/// 获取共享的 `FsOperations` 实例（对应 TS `getFsImplementation`）。
///
/// 由于 trait object 不能 Clone，这里返回一个静态默认实现的引用；当通过
/// [`set_fs_implementation`] 注入了自定义实现时，调用方可以改为使用
/// `with_fs_implementation` 等高阶函数（未来扩展），保持 API parity。
pub fn get_fs_implementation() -> &'static NodeFsOperationsImpl {
    let _ = ensure_default();
    &NODE_FS_OPERATIONS
}

/// 设置原始 `FsOperations`（对应 TS `setOriginalFsImplementation`）。
pub fn set_original_fs_implementation(impl_: Box<dyn FsOperationsTrait>) {
    let cell = FS_ORIGINAL_IMPL.get_or_init(|| Mutex::new(Box::new(NodeFsOperationsImpl)));
    *cell.lock().unwrap() = impl_;
}

/// 同步版本：从绝对路径向上查找第一个存在的祖先。
///
/// 对应 TS `resolveDeepestExistingAncestorSync`。这里复用现有的同步实现
/// [`resolve_deepest_existing_ancestor`]。
pub fn resolve_deepest_existing_ancestor_sync(absolute_path: &Path) -> Option<PathBuf> {
    resolve_deepest_existing_ancestor(absolute_path)
}
