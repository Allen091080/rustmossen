use std::collections::{HashMap, HashSet};

/// A tree node can contain other nodes or leaf string values.
#[derive(Debug, Clone)]
pub enum TreeValue {
    Node(HashMap<String, TreeValue>),
    Leaf(String),
}

/// 对应 TS `TreeNode = { [key: string]: TreeNode | string | undefined }`。
/// Rust 端的 `TreeNode` 直接采用 `HashMap<String, TreeValue>`。
pub type TreeNode = HashMap<String, TreeValue>;

/// Options for tree rendering.
#[derive(Debug, Clone, Default)]
pub struct TreeifyOptions {
    pub show_values: bool,
    pub hide_functions: bool,
}

/// Tree drawing characters.
struct TreeCharacters {
    branch: &'static str,
    last_branch: &'static str,
    line: &'static str,
    empty: &'static str,
}

const DEFAULT_TREE_CHARS: TreeCharacters = TreeCharacters {
    branch: "\u{251c}",      // ├
    last_branch: "\u{2514}", // └
    line: "\u{2502}",        // │
    empty: " ",
};

/// Custom treeify implementation.
pub fn treeify(obj: &HashMap<String, TreeValue>, options: Option<TreeifyOptions>) -> String {
    let opts = options.unwrap_or(TreeifyOptions {
        show_values: true,
        hide_functions: false,
    });

    let keys: Vec<&String> = obj.keys().collect();
    if keys.is_empty() {
        return "(empty)".to_string();
    }

    // Special case for single empty/whitespace string key
    if keys.len() == 1 {
        let key = keys[0];
        if key.trim().is_empty() {
            if let Some(TreeValue::Leaf(val)) = obj.get(key) {
                return format!("{} {}", DEFAULT_TREE_CHARS.last_branch, val);
            }
        }
    }

    let mut lines: Vec<String> = Vec::new();
    let mut visited: HashSet<*const HashMap<String, TreeValue>> = HashSet::new();

    grow_branch(obj, "", true, 0, &opts, &mut lines, &mut visited);
    lines.join("\n")
}

fn grow_branch(
    node: &HashMap<String, TreeValue>,
    prefix: &str,
    _is_last: bool,
    depth: usize,
    opts: &TreeifyOptions,
    lines: &mut Vec<String>,
    visited: &mut HashSet<*const HashMap<String, TreeValue>>,
) {
    let ptr = node as *const HashMap<String, TreeValue>;
    if visited.contains(&ptr) {
        lines.push(format!("{}[Circular]", prefix));
        return;
    }
    visited.insert(ptr);

    let keys: Vec<&String> = node.keys().collect();

    for (index, key) in keys.iter().enumerate() {
        let value = &node[*key];
        let is_last_key = index == keys.len() - 1;
        let node_prefix = if depth == 0 && index == 0 { "" } else { prefix };

        let tree_char = if is_last_key {
            DEFAULT_TREE_CHARS.last_branch
        } else {
            DEFAULT_TREE_CHARS.branch
        };

        let key_display = if key.trim().is_empty() {
            String::new()
        } else {
            key.to_string()
        };

        let mut line = format!(
            "{}{}{}",
            node_prefix,
            tree_char,
            if key_display.is_empty() {
                String::new()
            } else {
                format!(" {}", key_display)
            }
        );

        let should_add_colon = !key.trim().is_empty();

        match value {
            TreeValue::Node(child) => {
                let child_ptr = child as *const HashMap<String, TreeValue>;
                if visited.contains(&child_ptr) {
                    let separator = if should_add_colon {
                        ": "
                    } else if !line.is_empty() {
                        " "
                    } else {
                        ""
                    };
                    lines.push(format!("{}{}[Circular]", line, separator));
                } else {
                    lines.push(line);
                    let continuation_char = if is_last_key {
                        DEFAULT_TREE_CHARS.empty
                    } else {
                        DEFAULT_TREE_CHARS.line
                    };
                    let next_prefix = format!("{}{} ", node_prefix, continuation_char);
                    grow_branch(
                        child,
                        &next_prefix,
                        is_last_key,
                        depth + 1,
                        opts,
                        lines,
                        visited,
                    );
                }
            }
            TreeValue::Leaf(val) => {
                if opts.show_values {
                    let separator = if should_add_colon {
                        ": "
                    } else if !line.is_empty() {
                        " "
                    } else {
                        ""
                    };
                    line = format!("{}{}{}", line, separator, val);
                }
                lines.push(line);
            }
        }
    }

    visited.remove(&ptr);
}
