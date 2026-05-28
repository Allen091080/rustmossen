/// Brief/SendUserMessage tool name.
pub const BRIEF_TOOL_NAME: &str = "SendUserMessage";
/// Legacy brief tool name.
pub const LEGACY_BRIEF_TOOL_NAME: &str = "Brief";

/// Tool description.
pub const DESCRIPTION: &str = "Send a message to the user";

/// Brief tool prompt.
pub const BRIEF_TOOL_PROMPT: &str = "Send a message the user will read. Text outside this tool is visible in the detail view, but most won't open it \u{2014} the answer lives here.\n\n\
`message` supports markdown. `attachments` takes file paths (absolute or cwd-relative) for images, diffs, logs.\n\
`status` labels intent: 'normal' when replying to what they just asked; 'proactive' when you're initiating \u{2014} a scheduled task finished, a blocker surfaced during background work, you need input on something they haven't asked about. Set it honestly; downstream routing uses it.";

/// Brief proactive section prompt.
pub fn brief_proactive_section() -> String {
    format!(
        "## Talking to the user\n\n\
         {name} is where your replies go. Text outside it is visible if the user expands the detail view, \
         but most won't \u{2014} assume unread. Anything you want them to actually see goes through {name}. \
         The failure mode: the real answer lives in plain text while {name} just says \"done!\" \u{2014} \
         they see \"done!\" and miss everything.\n\n\
         So: every time the user says something, the reply they actually read comes through {name}. \
         Even for \"hi\". Even for \"thanks\".\n\n\
         If you can answer right away, send the answer. If you need to go look \u{2014} run a command, \
         read files, check something \u{2014} ack first in one line (\"On it \u{2014} checking the test output\"), \
         then work, then send the result. Without the ack they're staring at a spinner.\n\n\
         For longer work: ack \u{2192} work \u{2192} result. Between those, send a checkpoint when something \
         useful happened \u{2014} a decision you made, a surprise you hit, a phase boundary. Skip the filler \
         (\"running tests...\") \u{2014} a checkpoint earns its place by carrying information.\n\n\
         Keep messages tight \u{2014} the decision, the file:line, the PR number. Second person always \
         (\"your config\"), never third.",
        name = BRIEF_TOOL_NAME,
    )
}
