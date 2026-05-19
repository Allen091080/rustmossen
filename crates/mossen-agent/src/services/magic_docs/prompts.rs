//! Magic docs prompts

/// System prompt for documentation generation
pub const MAGIC_DOCS_SYSTEM_PROMPT: &str = r#"You are a documentation generation agent. Your job is to analyze code and produce clear, useful documentation.

Guidelines:
- Focus on the "why" not just the "what"
- Include usage examples where helpful
- Document public APIs, important types, and key concepts
- Note any gotchas, limitations, or important caveats
- Keep documentation concise but comprehensive
- Use standard doc comment format for the target language"#;

/// Build prompt for generating docs for a file/module
pub fn build_magic_docs_prompt(
    file_path: &str,
    file_content: &str,
    context: Option<&str>,
) -> String {
    let mut prompt = format!(
        "{}\n\nGenerate documentation for the following file: {}\n\n```\n{}\n```\n",
        MAGIC_DOCS_SYSTEM_PROMPT, file_path, file_content
    );

    if let Some(ctx) = context {
        prompt.push_str(&format!(
            "\nAdditional context about this code:\n{}\n",
            ctx
        ));
    }

    prompt.push_str(
        "\nOutput documentation in the appropriate format for this file type. \
         Include module-level docs and function/type docs where applicable.",
    );

    prompt
}

/// TS `buildMagicDocsUpdatePrompt` — variant of the prompt used when updating
/// existing docs rather than generating from scratch.
pub fn build_magic_docs_update_prompt(
    existing_doc: &str,
    file_path: &str,
    file_content: &str,
) -> String {
    format!(
        "{prelude}\n\nFile: {file_path}\n\nExisting documentation:\n{existing_doc}\n\nUpdated file content:\n```\n{file_content}\n```\n\nUpdate the documentation above to reflect the file's current contents. Keep accurate sections, rewrite stale ones.",
        prelude = MAGIC_DOCS_SYSTEM_PROMPT,
        existing_doc = existing_doc,
        file_path = file_path,
        file_content = file_content,
    )
}
