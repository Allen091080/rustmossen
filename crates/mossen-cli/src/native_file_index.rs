//! 文件索引 — 对应 TS 的 native-ts/file-index/index.ts。
//!
//! 高性能模糊文件搜索，模拟 nucleo（helix-editor）的 API 和评分行为。
//! 纯 Rust 实现，无需原生依赖。

use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// 搜索结果。
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub score: f64,
}

// ---------------------------------------------------------------------------
// Scoring constants (nucleo-style, approximating fzf-v2 / nucleo bonuses)
// ---------------------------------------------------------------------------

const SCORE_MATCH: i64 = 16;
const BONUS_BOUNDARY: i64 = 8;
const BONUS_CAMEL: i64 = 6;
const BONUS_CONSECUTIVE: i64 = 4;
const BONUS_FIRST_CHAR: i64 = 8;
const PENALTY_GAP_START: i64 = 3;
const PENALTY_GAP_EXTENSION: i64 = 1;

const TOP_LEVEL_CACHE_LIMIT: usize = 100;
const MAX_QUERY_LEN: usize = 64;

// ---------------------------------------------------------------------------
// FileIndex
// ---------------------------------------------------------------------------

/// 文件索引，支持模糊搜索。
pub struct FileIndex {
    paths: Vec<String>,
    lower_paths: Vec<String>,
    char_bits: Vec<i32>,
    path_lens: Vec<u16>,
    top_level_cache: Option<Vec<SearchResult>>,
}

impl FileIndex {
    /// 创建空索引。
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            lower_paths: Vec::new(),
            char_bits: Vec::new(),
            path_lens: Vec::new(),
            top_level_cache: None,
        }
    }

    /// 从路径列表加载索引。自动去重。
    pub fn load_from_file_list(&mut self, file_list: &[String]) {
        let mut seen = HashSet::new();
        let mut paths = Vec::new();
        for line in file_list {
            if !line.is_empty() && seen.insert(line.clone()) {
                paths.push(line.clone());
            }
        }
        self.build_index(paths);
    }

    /// 模糊搜索文件。返回最多 limit 个结果，按匹配分数排序。
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        if limit == 0 {
            return Vec::new();
        }
        if query.is_empty() {
            if let Some(ref cache) = self.top_level_cache {
                return cache.iter().take(limit).cloned().collect();
            }
            return Vec::new();
        }

        // Smart case: 全小写查询 → 大小写不敏感；含大写 → 敏感
        let case_sensitive = query != query.to_lowercase();
        let needle: String = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };
        let n_len = needle.len().min(MAX_QUERY_LEN);
        let needle_chars: Vec<char> = needle.chars().take(n_len).collect();
        let mut needle_bitmap: i32 = 0;
        for &ch in &needle_chars {
            let cc = ch as u32;
            if cc >= 97 && cc <= 122 {
                needle_bitmap |= 1 << (cc - 97);
            }
        }

        let score_ceiling = (n_len as i64) * (SCORE_MATCH + BONUS_BOUNDARY) + BONUS_FIRST_CHAR + 32;

        // Top-k: 维护按分数升序排列的 best matches
        let mut top_k: Vec<(String, i64)> = Vec::new();
        let mut threshold = i64::MIN;

        let ready_count = self.paths.len();

        'outer: for i in 0..ready_count {
            // O(1) bitmap 排除：路径必须包含 needle 中的每个字母
            if (self.char_bits[i] & needle_bitmap) != needle_bitmap {
                continue;
            }

            let haystack: &str = if case_sensitive {
                &self.paths[i]
            } else {
                &self.lower_paths[i]
            };

            // 顺序查找每个 needle 字符的位置
            let mut pos_buf = vec![0i32; n_len];
            let hay_chars: Vec<char> = haystack.chars().collect();
            let hay_len = hay_chars.len();

            // 找第一个字符
            let first_pos = match hay_chars.iter().position(|&c| c == needle_chars[0]) {
                Some(p) => p,
                None => continue,
            };
            pos_buf[0] = first_pos as i32;

            let mut gap_penalty: i64 = 0;
            let mut consec_bonus: i64 = 0;
            let mut prev = first_pos;

            for j in 1..n_len {
                let search_start = prev + 1;
                if search_start >= hay_len {
                    continue 'outer;
                }
                match hay_chars[search_start..]
                    .iter()
                    .position(|&c| c == needle_chars[j])
                {
                    Some(rel_pos) => {
                        let abs_pos = search_start + rel_pos;
                        pos_buf[j] = abs_pos as i32;
                        let gap = abs_pos - prev - 1;
                        if gap == 0 {
                            consec_bonus += BONUS_CONSECUTIVE;
                        } else {
                            gap_penalty += PENALTY_GAP_START + (gap as i64) * PENALTY_GAP_EXTENSION;
                        }
                        prev = abs_pos;
                    }
                    None => continue 'outer,
                }
            }

            // gap-bound 排除
            if top_k.len() == limit && score_ceiling + consec_bonus - gap_penalty <= threshold {
                continue;
            }

            // 边界/驼峰评分
            let path = &self.paths[i];
            let path_chars: Vec<char> = path.chars().collect();
            let h_len = self.path_lens[i] as i64;
            let mut score = (n_len as i64) * SCORE_MATCH + consec_bonus - gap_penalty;
            score += score_bonus_at(&path_chars, pos_buf[0] as usize, true);
            for j in 1..n_len {
                score += score_bonus_at(&path_chars, pos_buf[j] as usize, false);
            }
            score += (32 - (h_len >> 2)).max(0);

            if top_k.len() < limit {
                top_k.push((path.clone(), score));
                if top_k.len() == limit {
                    top_k.sort_by_key(|x| x.1);
                    threshold = top_k[0].1;
                }
            } else if score > threshold {
                // 二分插入
                let insert_pos = top_k.partition_point(|x| x.1 < score);
                top_k.insert(insert_pos, (path.clone(), score));
                top_k.remove(0);
                threshold = top_k[0].1;
            }
        }

        // 降序排列（最佳匹配优先）
        top_k.sort_by(|a, b| b.1.cmp(&a.1));

        let match_count = top_k.len();
        let denom = match_count.max(1) as f64;

        top_k
            .iter()
            .enumerate()
            .map(|(i, (path, _))| {
                let position_score = i as f64 / denom;
                let final_score = if path.contains("test") {
                    (position_score * 1.05).min(1.0)
                } else {
                    position_score
                };
                SearchResult {
                    path: path.clone(),
                    score: final_score,
                }
            })
            .collect()
    }

    // ---- Internal ----

    fn build_index(&mut self, paths: Vec<String>) {
        let n = paths.len();
        self.paths = paths;
        self.lower_paths = Vec::with_capacity(n);
        self.char_bits = vec![0; n];
        self.path_lens = vec![0; n];
        self.top_level_cache = compute_top_level_entries(&self.paths, TOP_LEVEL_CACHE_LIMIT);

        for i in 0..n {
            self.index_path(i);
        }
    }

    fn index_path(&mut self, i: usize) {
        let lp = self.paths[i].to_lowercase();
        let len = lp.len().min(u16::MAX as usize) as u16;
        self.path_lens[i] = len;
        let mut bits: i32 = 0;
        for c in lp.bytes() {
            if c >= b'a' && c <= b'z' {
                bits |= 1 << (c - b'a');
            }
        }
        self.char_bits[i] = bits;
        self.lower_paths.push(lp);
    }
}

impl Default for FileIndex {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn score_bonus_at(path_chars: &[char], pos: usize, first: bool) -> i64 {
    if pos == 0 {
        return if first { BONUS_FIRST_CHAR } else { 0 };
    }
    if pos >= path_chars.len() {
        return 0;
    }
    let prev_ch = path_chars[pos - 1];
    if is_boundary(prev_ch) {
        return BONUS_BOUNDARY;
    }
    if prev_ch.is_lowercase() && path_chars[pos].is_uppercase() {
        return BONUS_CAMEL;
    }
    0
}

fn is_boundary(ch: char) -> bool {
    matches!(ch, '/' | '\\' | '-' | '_' | '.' | ' ')
}

/// 提取唯一的顶层路径段，按 (长度升序, 字母升序) 排列。
fn compute_top_level_entries(paths: &[String], limit: usize) -> Option<Vec<SearchResult>> {
    let mut top_level = HashSet::new();

    for p in paths {
        let end = p.find(|c: char| c == '/' || c == '\\').unwrap_or(p.len());
        let segment = &p[..end];
        if !segment.is_empty() {
            top_level.insert(segment.to_string());
            if top_level.len() >= limit {
                break;
            }
        }
    }

    let mut sorted: Vec<String> = top_level.into_iter().collect();
    sorted.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));

    Some(
        sorted
            .into_iter()
            .take(limit)
            .map(|path| SearchResult { path, score: 0.0 })
            .collect(),
    )
}
