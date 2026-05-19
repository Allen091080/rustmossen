//! Consolidation prompt — prompt template for the memory consolidation agent.

/// Build the consolidation prompt for the auto-dream agent.
pub fn build_consolidation_prompt(
    memory_files: &[(String, String)],
    max_files: usize,
) -> String {
    let file_list = memory_files
        .iter()
        .take(max_files)
        .map(|(path, content)| format!("### {}\n\n{}", path, content))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    format!(
        r#"You are a memory consolidation agent. Your job is to organize and consolidate the user's memory files.

## Instructions

1. Review the memory files below
2. Identify duplicates, conflicts, and opportunities to merge
3. Consolidate related memories into fewer, higher-quality files
4. Remove outdated or superseded information
5. Keep the total number of memories manageable

## Guidelines

- Merge memories that cover the same topic into a single comprehensive file
- Remove memories that are clearly outdated or contradicted by newer ones
- Preserve all unique, non-redundant information
- Keep file names descriptive and kebab-case
- Do not create new information — only reorganize existing content
- Prefer fewer, richer files over many small fragments

## Current Memory Files

{file_list}

## Output

For each action, use the appropriate file tool:
- To merge files: Write the merged content to one file, then delete the other(s)
- To update: Edit the file with improved content
- To delete: Remove the file

If no consolidation is needed, output nothing."#,
        file_list = file_list
    )
}
