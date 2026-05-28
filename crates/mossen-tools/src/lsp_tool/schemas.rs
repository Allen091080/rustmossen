use serde::Deserialize;

/// Valid LSP operation types.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum LspOperation {
    GoToDefinition,
    FindReferences,
    Hover,
    DocumentSymbol,
    WorkspaceSymbol,
    GoToImplementation,
    PrepareCallHierarchy,
    IncomingCalls,
    OutgoingCalls,
}

/// LSP tool input.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspToolInput {
    pub operation: LspOperation,
    pub file_path: String,
    pub line: u32,
    pub character: u32,
}

/// Check if an operation string is a valid LSP operation.
pub fn is_valid_lsp_operation(operation: &str) -> bool {
    matches!(
        operation,
        "goToDefinition"
            | "findReferences"
            | "hover"
            | "documentSymbol"
            | "workspaceSymbol"
            | "goToImplementation"
            | "prepareCallHierarchy"
            | "incomingCalls"
            | "outgoingCalls"
    )
}

/// Returns the JSON input schema for the LSP tool.
pub fn input_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "discriminator": { "propertyName": "operation" },
        "oneOf": [
            operation_schema("goToDefinition"),
            operation_schema("findReferences"),
            operation_schema("hover"),
            operation_schema("documentSymbol"),
            operation_schema("workspaceSymbol"),
            operation_schema("goToImplementation"),
            operation_schema("prepareCallHierarchy"),
            operation_schema("incomingCalls"),
            operation_schema("outgoingCalls"),
        ]
    })
}

fn operation_schema(operation: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "operation": { "type": "string", "const": operation },
            "filePath": { "type": "string", "description": "The absolute or relative path to the file" },
            "line": { "type": "integer", "description": "The line number (1-based)" },
            "character": { "type": "integer", "description": "The character offset (1-based)" }
        },
        "required": ["operation", "filePath", "line", "character"],
        "additionalProperties": false
    })
}

/// Alias for the LSP tool input validator (mirrors TS `lspToolInputSchema`).
#[allow(non_camel_case_types)]
pub type lspToolInputSchema = LspToolInput;
