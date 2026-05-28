/// Web fetch tool name.
pub const WEB_FETCH_TOOL_NAME: &str = "WebFetch";

/// Web fetch tool description.
pub const DESCRIPTION: &str = "\
- Fetches content from a specified URL and processes it using an AI model\n\
- Takes a URL and a prompt as input\n\
- Fetches the URL content, converts HTML to markdown\n\
- Processes the content with the prompt using a small, fast model\n\
- Returns the model's response about the content\n\
- Use this tool when you need to retrieve and analyze web content\n\n\
Usage notes:\n\
  - IMPORTANT: If an MCP-provided web fetch tool is available, prefer using that tool instead of this one, as it may have fewer restrictions.\n\
  - The URL must be a fully-formed valid URL\n\
  - HTTP URLs will be automatically upgraded to HTTPS\n\
  - The prompt should describe what information you want to extract from the page\n\
  - This tool is read-only and does not modify any files\n\
  - Results may be summarized if the content is very large\n\
  - Includes a self-cleaning 15-minute cache for faster responses when repeatedly accessing the same URL\n\
  - When a URL redirects to a different host, the tool will inform you and provide the redirect URL in a special format. You should then make a new WebFetch request with the redirect URL to fetch the content.\n\
  - For GitHub URLs, prefer using the gh CLI via Bash instead (e.g., gh pr view, gh issue view, gh api).";

/// Build the prompt for the secondary model processing web content.
pub fn make_secondary_model_prompt(
    markdown_content: &str,
    prompt: &str,
    is_preapproved_domain: bool,
) -> String {
    let guidelines = if is_preapproved_domain {
        "Provide a concise response based on the content above. Include relevant details, \
         code examples, and documentation excerpts as needed."
    } else {
        "Provide a concise response based only on the content above. In your response:\n\
         - Enforce a strict 125-character maximum for quotes from any source document. \
         Open Source Software is ok as long as we respect the license.\n\
         - Use quotation marks for exact language from articles; any language outside of \
         the quotation should never be word-for-word the same.\n\
         - You are not a lawyer and never comment on the legality of your own prompts and responses.\n\
         - Never produce or reproduce exact song lyrics."
    };

    format!(
        "\nWeb page content:\n---\n{}\n---\n\n{}\n\n{}\n",
        markdown_content, prompt, guidelines
    )
}
