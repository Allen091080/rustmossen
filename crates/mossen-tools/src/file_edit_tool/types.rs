use serde::{Deserialize, Serialize};

/// Input for a single file edit operation.
#[derive(Debug, Clone, Deserialize)]
pub struct FileEditInput {
    /// The absolute path to the file to modify.
    pub file_path: String,
    /// The text to replace.
    pub old_string: String,
    /// The text to replace it with (must be different from old_string).
    pub new_string: String,
    /// Replace all occurrences of old_string (default false).
    #[serde(default)]
    pub replace_all: Option<bool>,
}

/// Individual edit without file_path.
#[derive(Debug, Clone, Deserialize)]
pub struct EditInput {
    pub old_string: String,
    pub new_string: String,
    #[serde(default)]
    pub replace_all: Option<bool>,
}

/// Runtime version where replace_all is always defined.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEdit {
    pub old_string: String,
    pub new_string: String,
    pub replace_all: bool,
}

/// A single diff hunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredPatchHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<String>,
}

/// Git diff information for a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiff {
    pub filename: String,
    pub status: GitDiffStatus,
    pub additions: usize,
    pub deletions: usize,
    pub changes: usize,
    pub patch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
}

/// Status of a git diff entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitDiffStatus {
    Modified,
    Added,
}

/// Output schema for FileEditTool.
#[derive(Debug, Clone, Serialize)]
pub struct FileEditOutput {
    pub file_path: String,
    pub old_string: String,
    pub new_string: String,
    pub original_file: String,
    pub structured_patch: Vec<StructuredPatchHunk>,
    pub user_modified: bool,
    pub replace_all: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_diff: Option<GitDiff>,
}

/// `FileEditTool/types.ts` `hunkSchema` — JSON schema fragment describing a
/// single diff hunk (oldStart, oldLines, newStart, newLines, lines).
pub fn hunk_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "oldStart": { "type": "number" },
            "oldLines": { "type": "number" },
            "newStart": { "type": "number" },
            "newLines": { "type": "number" },
            "lines": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["oldStart", "oldLines", "newStart", "newLines", "lines"]
    })
}

/// Alias matching the TS export name `hunkSchema` for cross-language symbol
/// parity. Calls the canonical Rust factory `hunk_schema()`.
#[allow(non_snake_case)]
pub fn hunkSchema() -> serde_json::Value {
    hunk_schema()
}

/// Returns the JSON input schema for the FileEditTool.
pub fn input_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "file_path": {
                "type": "string",
                "description": "The absolute path to the file to modify"
            },
            "old_string": {
                "type": "string",
                "description": "The text to replace"
            },
            "new_string": {
                "type": "string",
                "description": "The text to replace it with (must be different from old_string)"
            },
            "replace_all": {
                "type": "boolean",
                "description": "Replace all occurrences of old_string (default false)",
                "default": false
            }
        },
        "required": ["file_path", "old_string", "new_string"],
        "additionalProperties": false
    })
}

/// Returns the JSON output schema for the FileEditTool.
pub fn output_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "filePath": { "type": "string", "description": "The file path that was edited" },
            "oldString": { "type": "string", "description": "The original string that was replaced" },
            "newString": { "type": "string", "description": "The new string that replaced it" },
            "originalFile": { "type": "string", "description": "The original file contents before editing" },
            "structuredPatch": {
                "type": "array",
                "description": "Diff patch showing the changes",
                "items": {
                    "type": "object",
                    "properties": {
                        "oldStart": { "type": "number" },
                        "oldLines": { "type": "number" },
                        "newStart": { "type": "number" },
                        "newLines": { "type": "number" },
                        "lines": { "type": "array", "items": { "type": "string" } }
                    }
                }
            },
            "userModified": { "type": "boolean", "description": "Whether the user modified the proposed changes" },
            "replaceAll": { "type": "boolean", "description": "Whether all occurrences were replaced" },
            "gitDiff": {
                "type": "object",
                "description": "Git diff for the file (optional)"
            }
        }
    })
}

/// Alias for the git diff validator (mirrors TS `gitDiffSchema`).
#[allow(non_camel_case_types)]
pub type gitDiffSchema = GitDiff;
