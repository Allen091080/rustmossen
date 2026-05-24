use super::constants::{BASH_TOOL_NAME, MAX_LINES_TO_READ};
use super::limits::get_default_file_reading_limits;

/// Short description for the Read tool.
pub const DESCRIPTION: &str = "Read a file from the local filesystem.";

/// Line format instruction.
pub const LINE_FORMAT_INSTRUCTION: &str =
    "- Results are returned using cat -n format, with line numbers starting at 1";

/// Default offset instruction.
pub const OFFSET_INSTRUCTION_DEFAULT: &str =
    "- You can optionally specify a line offset and limit (especially handy for long files), \
     but it's recommended to read the whole file by not providing these parameters";

/// Targeted offset instruction for targeted range nudge.
pub const OFFSET_INSTRUCTION_TARGETED: &str =
    "- When you already know which part of the file you need, only read that part. \
     This can be important for larger files.";

/// Renders the Read tool prompt template.
pub fn render_prompt_template(
    line_format: &str,
    max_size_instruction: &str,
    offset_instruction: &str,
) -> String {
    let pdf_instruction = if is_pdf_supported() {
        format!(
            "\n- This tool can read PDF files (.pdf). For large PDFs (more than 10 pages), \
             you MUST provide the pages parameter to read specific page ranges (e.g., pages: \"1-5\"). \
             Reading a large PDF without the pages parameter will fail. Maximum 20 pages per request."
        )
    } else {
        String::new()
    };

    format!(
        "Reads a file from the local filesystem. You can access any file directly by using this tool.\n\
         Assume this tool is able to read all files on the machine. If the User provides a path to a file \
         assume that path is valid. It is okay to read a file that does not exist; an error will be returned.\n\n\
         Usage:\n\
         - The file_path parameter must be an absolute path, not a relative path\n\
         - By default, it reads up to {max_lines} lines starting from the beginning of the file{max_size}\n\
         {offset}\n\
         {line_format}\n\
         - This tool allows Mossen to read images (eg PNG, JPG, etc). When reading an image file the \
         contents are presented visually as Mossen is a multimodal LLM.{pdf}\n\
         - This tool can read Jupyter notebooks (.ipynb files) and returns all cells with their outputs, \
         combining code, text, and visualizations.\n\
         - This tool can only read files, not directories. To read a directory, use an ls command via the {bash} tool.\n\
         - You will regularly be asked to read screenshots. If the user provides a path to a screenshot, \
         ALWAYS use this tool to view the file at the path. This tool will work with all temporary file paths.\n\
         - If you read a file that exists but has empty contents you will receive a system reminder \
         warning in place of file contents.",
        max_lines = MAX_LINES_TO_READ,
        max_size = max_size_instruction,
        offset = offset_instruction,
        line_format = line_format,
        pdf = pdf_instruction,
        bash = BASH_TOOL_NAME,
    )
}

/// Build the full prompt for the Read tool.
pub fn build_read_tool_prompt() -> String {
    let limits = get_default_file_reading_limits();
    let max_size_instruction = if limits.include_max_size_in_prompt.unwrap_or(false) {
        format!(
            ". Files larger than {} will return an error; use offset and limit for larger files",
            format_file_size(limits.max_size_bytes)
        )
    } else {
        String::new()
    };
    let offset_instruction = if limits.targeted_range_nudge.unwrap_or(false) {
        OFFSET_INSTRUCTION_TARGETED
    } else {
        OFFSET_INSTRUCTION_DEFAULT
    };
    render_prompt_template(
        LINE_FORMAT_INSTRUCTION,
        &max_size_instruction,
        offset_instruction,
    )
}

/// Format file size for human display.
fn format_file_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Whether PDF reading is supported (check environment).
fn is_pdf_supported() -> bool {
    std::env::var("MOSSEN_PDF_SUPPORTED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Returns the JSON input schema for the FileReadTool.
pub fn input_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "file_path": {
                "type": "string",
                "description": "The absolute path to the file to read"
            },
            "offset": {
                "type": "integer",
                "description": "The line number to start reading from. Only provide if the file is too large to read at once"
            },
            "limit": {
                "type": "integer",
                "description": "The number of lines to read. Only provide if the file is too large to read at once."
            },
            "pages": {
                "type": "string",
                "description": "Page range for PDF files (e.g., \"1-5\", \"3\", \"10-20\"). Only applicable to PDF files. Maximum 20 pages per request."
            }
        },
        "required": ["file_path"],
        "additionalProperties": false
    })
}
