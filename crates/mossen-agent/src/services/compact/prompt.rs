//! Compact prompt generation — produces the system/user prompts used for conversation compaction.

use regex::Regex;

/// Aggressive no-tools preamble for the compaction prompt.
const NO_TOOLS_PREAMBLE: &str = "CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.\n\n\
- Do NOT use Read, Bash, Grep, Glob, Edit, Write, or ANY other tool.\n\
- You already have all the context you need in the conversation above.\n\
- Tool calls will be REJECTED and will waste your only turn — you will fail the task.\n\
- Your entire response must be plain text: an <analysis> block followed by a <summary> block.\n\n";

const DETAILED_ANALYSIS_INSTRUCTION_BASE: &str = "Before providing your final summary, wrap your analysis in <analysis> tags to organize your thoughts and ensure you've covered all necessary points. In your analysis process:\n\n\
1. Chronologically analyze each message and section of the conversation. For each section thoroughly identify:\n\
   - The user's explicit requests and intents\n\
   - Your approach to addressing the user's requests\n\
   - Key decisions, technical concepts and code patterns\n\
   - Specific details like:\n\
     - file names\n\
     - full code snippets\n\
     - function signatures\n\
     - file edits\n\
   - Errors that you ran into and how you fixed them\n\
   - Pay special attention to specific user feedback that you received, especially if the user told you to do something differently.\n\
2. Double-check for technical accuracy and completeness, addressing each required element thoroughly.";

const DETAILED_ANALYSIS_INSTRUCTION_PARTIAL: &str = "Before providing your final summary, wrap your analysis in <analysis> tags to organize your thoughts and ensure you've covered all necessary points. In your analysis process:\n\n\
1. Analyze the recent messages chronologically. For each section thoroughly identify:\n\
   - The user's explicit requests and intents\n\
   - Your approach to addressing the user's requests\n\
   - Key decisions, technical concepts and code patterns\n\
   - Specific details like:\n\
     - file names\n\
     - full code snippets\n\
     - function signatures\n\
     - file edits\n\
   - Errors that you ran into and how you fixed them\n\
   - Pay special attention to specific user feedback that you received, especially if the user told you to do something differently.\n\
2. Double-check for technical accuracy and completeness, addressing each required element thoroughly.";

const NO_TOOLS_TRAILER: &str = "\n\nREMINDER: Do NOT call any tools. Respond with plain text only — \
an <analysis> block followed by a <summary> block. \
Tool calls will be rejected and you will fail the task.";

/// Direction for partial compaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartialCompactDirection {
    /// Summarize messages after the pivot, keep earlier ones.
    From,
    /// Summarize messages before the pivot, keep later ones.
    UpTo,
}

/// Get the full compaction prompt with optional custom instructions.
pub fn get_compact_prompt(custom_instructions: Option<&str>) -> String {
    let base = format!(
        "{}Your task is to create a detailed summary of the conversation so far, paying close attention to the user's explicit requests and your previous actions.\n\
This summary should be thorough in capturing technical details, code patterns, and architectural decisions that would be essential for continuing development work without losing context.\n\n\
{}\n\n\
Your summary should include the following sections:\n\n\
1. Primary Request and Intent: Capture all of the user's explicit requests and intents in detail\n\
2. Key Technical Concepts: List all important technical concepts, technologies, and frameworks discussed.\n\
3. Files and Code Sections: Enumerate specific files and code sections examined, modified, or created. Pay special attention to the most recent messages and include full code snippets where applicable and include a summary of why this file read or edit is important.\n\
4. Errors and fixes: List all errors that you ran into, and how you fixed them. Pay special attention to specific user feedback that you received, especially if the user told you to do something differently.\n\
5. Problem Solving: Document problems solved and any ongoing troubleshooting efforts.\n\
6. All user messages: List ALL user messages that are not tool results. These are critical for understanding the users' feedback and changing intent.\n\
7. Pending Tasks: Outline any pending tasks that you have explicitly been asked to work on.\n\
8. Current Work: Describe in detail precisely what was being worked on immediately before this summary request, paying special attention to the most recent messages from both user and assistant. Include file names and code snippets where applicable.\n\
9. Optional Next Step: List the next step that you will take that is related to the most recent work you were doing. IMPORTANT: ensure that this step is DIRECTLY in line with the user's most recent explicit requests, and the task you were working on immediately before this summary request. If your last task was concluded, then only list next steps if they are explicitly in line with the users request. Do not start on tangential requests or really old requests that were already completed without confirming with the user first.\n\
                       If there is a next step, include direct quotes from the most recent conversation showing exactly what task you were working on and where you left off. This should be verbatim to ensure there's no drift in task interpretation.\n\n\
Please provide your summary based on the conversation so far, following this structure and ensuring precision and thoroughness in your response.\n\n\
There may be additional summarization instructions provided in the included context. If so, remember to follow these instructions when creating the above summary.",
        NO_TOOLS_PREAMBLE,
        DETAILED_ANALYSIS_INSTRUCTION_BASE,
    );

    let mut prompt = base;
    if let Some(instructions) = custom_instructions {
        if !instructions.trim().is_empty() {
            prompt.push_str(&format!("\n\nAdditional Instructions:\n{}", instructions));
        }
    }
    prompt.push_str(NO_TOOLS_TRAILER);
    prompt
}

/// Get the partial compaction prompt with optional custom instructions.
pub fn get_partial_compact_prompt(
    custom_instructions: Option<&str>,
    direction: PartialCompactDirection,
) -> String {
    let template = match direction {
        PartialCompactDirection::UpTo => {
            format!(
                "Your task is to create a detailed summary of this conversation. This summary will be placed at the start of a continuing session; newer messages that build on this context will follow after your summary (you do not see them here). Summarize thoroughly so that someone reading only your summary and then the newer messages can fully understand what happened and continue the work.\n\n\
{}\n\n\
Your summary should include the following sections:\n\n\
1. Primary Request and Intent: Capture the user's explicit requests and intents in detail\n\
2. Key Technical Concepts: List important technical concepts, technologies, and frameworks discussed.\n\
3. Files and Code Sections: Enumerate specific files and code sections examined, modified, or created. Include full code snippets where applicable and include a summary of why this file read or edit is important.\n\
4. Errors and fixes: List errors encountered and how they were fixed.\n\
5. Problem Solving: Document problems solved and any ongoing troubleshooting efforts.\n\
6. All user messages: List ALL user messages that are not tool results.\n\
7. Pending Tasks: Outline any pending tasks.\n\
8. Work Completed: Describe what was accomplished by the end of this portion.\n\
9. Context for Continuing Work: Summarize any context, decisions, or state that would be needed to understand and continue the work in subsequent messages.\n\n\
Please provide your summary following this structure, ensuring precision and thoroughness in your response.",
                DETAILED_ANALYSIS_INSTRUCTION_BASE,
            )
        }
        PartialCompactDirection::From => {
            format!(
                "Your task is to create a detailed summary of the RECENT portion of the conversation — the messages that follow earlier retained context. The earlier messages are being kept intact and do NOT need to be summarized. Focus your summary on what was discussed, learned, and accomplished in the recent messages only.\n\n\
{}\n\n\
Your summary should include the following sections:\n\n\
1. Primary Request and Intent: Capture the user's explicit requests and intents from the recent messages\n\
2. Key Technical Concepts: List important technical concepts, technologies, and frameworks discussed recently.\n\
3. Files and Code Sections: Enumerate specific files and code sections examined, modified, or created. Include full code snippets where applicable and include a summary of why this file read or edit is important.\n\
4. Errors and fixes: List errors encountered and how they were fixed.\n\
5. Problem Solving: Document problems solved and any ongoing troubleshooting efforts.\n\
6. All user messages: List ALL user messages from the recent portion that are not tool results.\n\
7. Pending Tasks: Outline any pending tasks from the recent messages.\n\
8. Current Work: Describe precisely what was being worked on immediately before this summary request.\n\
9. Optional Next Step: List the next step related to the most recent work. Include direct quotes from the most recent conversation.\n\n\
Please provide your summary based on the RECENT messages only (after the retained earlier context), following this structure and ensuring precision and thoroughness in your response.",
                DETAILED_ANALYSIS_INSTRUCTION_PARTIAL,
            )
        }
    };

    let mut prompt = format!("{}{}", NO_TOOLS_PREAMBLE, template);
    if let Some(instructions) = custom_instructions {
        if !instructions.trim().is_empty() {
            prompt.push_str(&format!("\n\nAdditional Instructions:\n{}", instructions));
        }
    }
    prompt.push_str(NO_TOOLS_TRAILER);
    prompt
}

/// Formats the compact summary by stripping the <analysis> drafting scratchpad
/// and replacing <summary> XML tags with readable section headers.
pub fn format_compact_summary(summary: &str) -> String {
    let mut formatted = summary.to_string();

    // Strip analysis section
    let analysis_re = Regex::new(r"(?s)<analysis>.*?</analysis>").unwrap();
    formatted = analysis_re.replace(&formatted, "").to_string();

    // Extract and format summary section
    let summary_re = Regex::new(r"(?s)<summary>(.*?)</summary>").unwrap();
    if let Some(caps) = summary_re.captures(&formatted) {
        let content = caps.get(1).map_or("", |m| m.as_str()).trim();
        formatted = summary_re
            .replace(&formatted, &format!("Summary:\n{}", content))
            .to_string();
    }

    // Clean up extra whitespace between sections
    let multi_newline_re = Regex::new(r"\n\n+").unwrap();
    formatted = multi_newline_re.replace_all(&formatted, "\n\n").to_string();

    formatted.trim().to_string()
}

/// Build the user summary message content that wraps the compaction summary.
pub fn get_compact_user_summary_message(
    summary: &str,
    suppress_follow_up_questions: bool,
    transcript_path: Option<&str>,
    recent_messages_preserved: Option<bool>,
) -> String {
    let formatted_summary = format_compact_summary(summary);

    let mut base_summary = format!(
        "This session is being continued from a previous conversation that ran out of context. \
The summary below covers the earlier portion of the conversation.\n\n{}",
        formatted_summary
    );

    if let Some(path) = transcript_path {
        base_summary.push_str(&format!(
            "\n\nIf you need specific details from before compaction (like exact code snippets, \
error messages, or content you generated), read the full transcript at: {}",
            path
        ));
    }

    if recent_messages_preserved == Some(true) {
        base_summary.push_str("\n\nRecent messages are preserved verbatim.");
    }

    if suppress_follow_up_questions {
        let continuation = format!(
            "{}\n\
Continue the conversation from where it left off without asking the user any further questions. \
Resume directly — do not acknowledge the summary, do not recap what was happening, do not preface \
with \"I'll continue\" or similar. Pick up the last task as if the break never happened.",
            base_summary
        );
        return continuation;
    }

    base_summary
}

/// Merges user-supplied custom instructions with hook-provided instructions.
/// User instructions come first; hook instructions are appended.
/// Empty strings normalize to None.
pub fn merge_hook_instructions(
    user_instructions: Option<&str>,
    hook_instructions: Option<&str>,
) -> Option<String> {
    match (
        user_instructions.filter(|s| !s.is_empty()),
        hook_instructions.filter(|s| !s.is_empty()),
    ) {
        (None, None) => None,
        (Some(u), None) => Some(u.to_string()),
        (None, Some(h)) => Some(h.to_string()),
        (Some(u), Some(h)) => Some(format!("{}\n\n{}", u, h)),
    }
}
