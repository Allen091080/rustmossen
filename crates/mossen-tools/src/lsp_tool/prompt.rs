/// LSP tool name.
pub const LSP_TOOL_NAME: &str = "LSP";

/// LSP tool description.
pub const DESCRIPTION: &str = "Interact with Language Server Protocol (LSP) servers to get code intelligence features.\n\n\
Supported operations:\n\
- goToDefinition: Find where a symbol is defined\n\
- findReferences: Find all references to a symbol\n\
- hover: Get hover information (documentation, type info) for a symbol\n\
- documentSymbol: Get all symbols (functions, classes, variables) in a document\n\
- workspaceSymbol: Search for symbols across the entire workspace\n\
- goToImplementation: Find implementations of an interface or abstract method\n\
- prepareCallHierarchy: Get call hierarchy item at a position (functions/methods)\n\
- incomingCalls: Find all functions/methods that call the function at a position\n\
- outgoingCalls: Find all functions/methods called by the function at a position\n\n\
All operations require:\n\
- filePath: The file to operate on\n\
- line: The line number (1-based, as shown in editors)\n\
- character: The character offset (1-based, as shown in editors)\n\n\
Note: LSP servers must be configured for the file type. If no server is available, an error will be returned.";
