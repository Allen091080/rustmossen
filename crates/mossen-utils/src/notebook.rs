//! Jupyter notebook reading and processing utilities.
//!
//! Reads .ipynb files, processes cells with outputs, and maps them to
//! tool result block parameters for the API.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const LARGE_OUTPUT_THRESHOLD: usize = 10000;

/// Image data from a notebook cell output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookOutputImage {
    pub image_data: String,
    pub media_type: String,
}

/// Processed cell output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookCellSourceOutput {
    pub output_type: String,
    pub text: Option<String>,
    pub image: Option<NotebookOutputImage>,
}

/// Processed notebook cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookCellSource {
    pub cell_type: String,
    pub source: String,
    pub execution_count: Option<u64>,
    pub cell_id: String,
    pub language: Option<String>,
    pub outputs: Option<Vec<NotebookCellSourceOutput>>,
}

/// Raw notebook cell from .ipynb JSON.
#[derive(Debug, Clone, Deserialize)]
pub struct NotebookCell {
    pub cell_type: String,
    pub source: serde_json::Value,
    pub id: Option<String>,
    pub execution_count: Option<u64>,
    pub outputs: Option<Vec<NotebookCellOutput>>,
}

/// Raw notebook cell output.
#[derive(Debug, Clone, Deserialize)]
pub struct NotebookCellOutput {
    pub output_type: String,
    pub text: Option<serde_json::Value>,
    pub data: Option<serde_json::Map<String, serde_json::Value>>,
    pub ename: Option<String>,
    pub evalue: Option<String>,
    pub traceback: Option<Vec<String>>,
}

/// Notebook language info metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct LanguageInfo {
    pub name: Option<String>,
}

/// Notebook metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct NotebookMetadata {
    pub language_info: Option<LanguageInfo>,
}

/// Root notebook content.
#[derive(Debug, Clone, Deserialize)]
pub struct NotebookContent {
    pub cells: Vec<NotebookCell>,
    pub metadata: NotebookMetadata,
}

/// Text block for tool results.
#[derive(Debug, Clone, Serialize)]
pub struct TextBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
}

/// Image block for tool results.
#[derive(Debug, Clone, Serialize)]
pub struct ImageBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub source: ImageSource,
}

/// Image source.
#[derive(Debug, Clone, Serialize)]
pub struct ImageSource {
    pub data: String,
    pub media_type: String,
    #[serde(rename = "type")]
    pub source_type: String,
}

/// Tool result content block (text or image).
#[derive(Debug, Clone)]
pub enum ToolResultBlock {
    Text(TextBlock),
    Image(ImageBlock),
}

/// Tool result block param.
#[derive(Debug, Clone)]
pub struct ToolResultBlockParam {
    pub tool_use_id: String,
    pub content: Vec<ToolResultBlock>,
}

fn is_large_outputs(outputs: &[Option<NotebookCellSourceOutput>]) -> bool {
    let mut size = 0usize;
    for o in outputs {
        if let Some(output) = o {
            size += output.text.as_ref().map_or(0, |t| t.len());
            size += output.image.as_ref().map_or(0, |img| img.image_data.len());
            if size > LARGE_OUTPUT_THRESHOLD {
                return true;
            }
        }
    }
    false
}

fn process_output_text(text: &serde_json::Value) -> String {
    match text {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn extract_image(data: &serde_json::Map<String, Value>) -> Option<NotebookOutputImage> {
    if let Some(Value::String(png)) = data.get("image/png") {
        return Some(NotebookOutputImage {
            image_data: png.chars().filter(|c| !c.is_whitespace()).collect(),
            media_type: "image/png".to_string(),
        });
    }
    if let Some(Value::String(jpeg)) = data.get("image/jpeg") {
        return Some(NotebookOutputImage {
            image_data: jpeg.chars().filter(|c| !c.is_whitespace()).collect(),
            media_type: "image/jpeg".to_string(),
        });
    }
    None
}

fn process_output(output: &NotebookCellOutput) -> Option<NotebookCellSourceOutput> {
    match output.output_type.as_str() {
        "stream" => {
            let text = output
                .text
                .as_ref()
                .map(|t| process_output_text(t))
                .unwrap_or_default();
            Some(NotebookCellSourceOutput {
                output_type: output.output_type.clone(),
                text: Some(text),
                image: None,
            })
        }
        "execute_result" | "display_data" => {
            let text = output.data.as_ref().and_then(|d| {
                d.get("text/plain").map(|v| process_output_text(v))
            });
            let image = output.data.as_ref().and_then(|d| extract_image(d));
            Some(NotebookCellSourceOutput {
                output_type: output.output_type.clone(),
                text,
                image,
            })
        }
        "error" => {
            let ename = output.ename.as_deref().unwrap_or("");
            let evalue = output.evalue.as_deref().unwrap_or("");
            let traceback = output
                .traceback
                .as_ref()
                .map(|t| t.join("\n"))
                .unwrap_or_default();
            let text = format!("{ename}: {evalue}\n{traceback}");
            Some(NotebookCellSourceOutput {
                output_type: output.output_type.clone(),
                text: Some(text),
                image: None,
            })
        }
        _ => None,
    }
}

fn get_cell_source(source: &serde_json::Value) -> String {
    match source {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn process_cell(
    cell: &NotebookCell,
    index: usize,
    code_language: &str,
    include_large_outputs: bool,
    bash_tool_name: &str,
    notebook_path: &str,
) -> NotebookCellSource {
    let cell_id = cell
        .id
        .clone()
        .unwrap_or_else(|| format!("cell-{index}"));

    let mut cell_data = NotebookCellSource {
        cell_type: cell.cell_type.clone(),
        source: get_cell_source(&cell.source),
        execution_count: if cell.cell_type == "code" {
            cell.execution_count
        } else {
            None
        },
        cell_id,
        language: if cell.cell_type == "code" {
            Some(code_language.to_string())
        } else {
            None
        },
        outputs: None,
    };

    if cell.cell_type == "code" {
        if let Some(ref outputs) = cell.outputs {
            if !outputs.is_empty() {
                let processed: Vec<Option<NotebookCellSourceOutput>> =
                    outputs.iter().map(|o| process_output(o)).collect();

                if !include_large_outputs && is_large_outputs(&processed) {
                    cell_data.outputs = Some(vec![NotebookCellSourceOutput {
                        output_type: "stream".to_string(),
                        text: Some(format!(
                            "Outputs are too large to include. Use {bash_tool_name} with: cat {notebook_path} | jq '.cells[{index}].outputs'"
                        )),
                        image: None,
                    }]);
                } else {
                    cell_data.outputs =
                        Some(processed.into_iter().flatten().collect());
                }
            }
        }
    }

    cell_data
}

fn cell_content_to_tool_result(cell: &NotebookCellSource) -> TextBlock {
    let mut metadata = Vec::new();
    if cell.cell_type != "code" {
        metadata.push(format!("<cell_type>{}</cell_type>", cell.cell_type));
    }
    if cell.language.as_deref() != Some("python") && cell.cell_type == "code" {
        if let Some(ref lang) = cell.language {
            metadata.push(format!("<language>{lang}</language>"));
        }
    }
    let cell_content = format!(
        "<cell id=\"{}\">{}{}</cell id=\"{}\">",
        cell.cell_id,
        metadata.join(""),
        cell.source,
        cell.cell_id
    );
    TextBlock {
        block_type: "text".to_string(),
        text: cell_content,
    }
}

fn cell_output_to_tool_result(output: &NotebookCellSourceOutput) -> Vec<ToolResultBlock> {
    let mut results = Vec::new();
    if let Some(ref text) = output.text {
        results.push(ToolResultBlock::Text(TextBlock {
            block_type: "text".to_string(),
            text: format!("\n{text}"),
        }));
    }
    if let Some(ref image) = output.image {
        results.push(ToolResultBlock::Image(ImageBlock {
            block_type: "image".to_string(),
            source: ImageSource {
                data: image.image_data.clone(),
                media_type: image.media_type.clone(),
                source_type: "base64".to_string(),
            },
        }));
    }
    results
}

fn get_tool_result_from_cell(cell: &NotebookCellSource) -> Vec<ToolResultBlock> {
    let content_result = ToolResultBlock::Text(cell_content_to_tool_result(cell));
    let mut results = vec![content_result];
    if let Some(ref outputs) = cell.outputs {
        for output in outputs {
            results.extend(cell_output_to_tool_result(output));
        }
    }
    results
}

/// Reads and parses a Jupyter notebook file into processed cell data.
pub async fn read_notebook(
    notebook_path: &str,
    cell_id: Option<&str>,
    bash_tool_name: &str,
) -> anyhow::Result<Vec<NotebookCellSource>> {
    let full_path = shellexpand::tilde(notebook_path).to_string();
    let content = tokio::fs::read_to_string(&full_path).await?;
    let notebook: NotebookContent = serde_json::from_str(&content)?;
    let language = notebook
        .metadata
        .language_info
        .as_ref()
        .and_then(|li| li.name.as_deref())
        .unwrap_or("python");

    if let Some(cid) = cell_id {
        let (idx, cell) = notebook
            .cells
            .iter()
            .enumerate()
            .find(|(_, c)| c.id.as_deref() == Some(cid))
            .ok_or_else(|| {
                anyhow::anyhow!("Cell with ID \"{}\" not found in notebook", cid)
            })?;
        return Ok(vec![process_cell(
            cell,
            idx,
            language,
            true,
            bash_tool_name,
            notebook_path,
        )]);
    }

    Ok(notebook
        .cells
        .iter()
        .enumerate()
        .map(|(i, cell)| process_cell(cell, i, language, false, bash_tool_name, notebook_path))
        .collect())
}

/// Maps notebook cell data to tool result block parameters with text block merging.
pub fn map_notebook_cells_to_tool_result(
    data: &[NotebookCellSource],
    tool_use_id: &str,
) -> ToolResultBlockParam {
    let all_results: Vec<ToolResultBlock> =
        data.iter().flat_map(|c| get_tool_result_from_cell(c)).collect();

    // Merge adjacent text blocks
    let merged = all_results.into_iter().fold(
        Vec::<ToolResultBlock>::new(),
        |mut acc, curr| {
            if acc.is_empty() {
                acc.push(curr);
                return acc;
            }
            match (&mut acc.last_mut(), &curr) {
                (Some(ToolResultBlock::Text(prev)), ToolResultBlock::Text(curr_text)) => {
                    prev.text.push('\n');
                    prev.text.push_str(&curr_text.text);
                }
                _ => {
                    acc.push(curr);
                }
            }
            acc
        },
    );

    ToolResultBlockParam {
        tool_use_id: tool_use_id.to_string(),
        content: merged,
    }
}

/// Parse a cell ID string like "cell-3" into its index.
pub fn parse_cell_id(cell_id: &str) -> Option<usize> {
    cell_id
        .strip_prefix("cell-")
        .and_then(|s| s.parse::<usize>().ok())
}
