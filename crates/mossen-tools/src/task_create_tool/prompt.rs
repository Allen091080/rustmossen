//! TaskCreateTool prompt.
//!
//! Translated from tools/TaskCreateTool/prompt.ts

pub const DESCRIPTION: &str = "Create a new task in the task list";

/// Returns the TaskCreate prompt with language-aware header and optional swarm context.
pub fn get_prompt(language_tag: &str, agent_swarms_enabled: bool) -> String {
    let lang_header = if language_tag == "zh" {
        "当前运行态语言为中文。若用户当前在用中文交流，请优先用中文撰写任务的 subject、description 和 activeForm；只有代码、命令、路径和必须保留的专有名词保持英文。"
    } else {
        "The current runtime language is English. Prefer English for task subject, description, and activeForm unless the user is clearly operating in another language."
    };

    let teammate_context = if agent_swarms_enabled {
        " and potentially assigned to teammates"
    } else {
        ""
    };

    let teammate_tips = if agent_swarms_enabled {
        "- Include enough detail in the description for another agent to understand and complete the task\n- New tasks are created with status 'pending' and no owner - use TaskUpdate with the `owner` parameter to assign them\n"
    } else {
        ""
    };

    format!(
        r#"{lang_header}

Use this tool to create a structured task list for your current coding session. This helps you track progress, organize complex tasks, and demonstrate thoroughness to the user.
It also helps the user understand the progress of the task and overall progress of their requests.

## When to Use This Tool

Use this tool proactively in these scenarios:

- Complex multi-step tasks - When a task requires 3 or more distinct steps or actions
- Non-trivial and complex tasks - Tasks that require careful planning or multiple operations{teammate_context}
- Plan mode - When using plan mode, create a task list to track the work
- User explicitly requests todo list - When the user directly asks you to use the todo list
- User provides multiple tasks - When users provide a list of things to be done (numbered or comma-separated)
- After receiving new instructions - Immediately capture user requirements as tasks
- When you start working on a task - Mark it as in_progress BEFORE beginning work
- After completing a task - Mark it as completed and add any new follow-up tasks discovered during implementation
- While working through a multi-step request - Keep the checklist fresh by marking each finished task as completed before moving on

## When NOT to Use This Tool

Skip using this tool when:
- There is only a single, straightforward task
- The task is trivial and tracking it provides no organizational benefit
- The task can be completed in less than 3 trivial steps
- The task is purely conversational or informational

NOTE that you should not use this tool if there is only one trivial task to do. In this case you are better off just doing the task directly.

## Task Fields

- **subject**: A brief, actionable title in imperative form (e.g., "Fix authentication bug in login flow")
- **description**: What needs to be done
- **activeForm** (optional): Present continuous form shown in the spinner when the task is in_progress (e.g., "Fixing authentication bug"). If omitted, the spinner shows the subject instead.

All tasks are created with status `pending`.

## Tips

- Create tasks with clear, specific subjects that describe the outcome
- After creating tasks, use TaskUpdate to set up dependencies (blocks/blockedBy) if needed
{teammate_tips}- Check TaskList first to avoid creating duplicate tasks
"#,
        lang_header = lang_header,
        teammate_context = teammate_context,
        teammate_tips = teammate_tips,
    )
}
