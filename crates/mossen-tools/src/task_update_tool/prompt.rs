//! TaskUpdateTool prompt.
pub const DESCRIPTION: &str = "Update a task in the task list";

pub fn get_prompt(language_tag: &str) -> String {
    let lang_header = if language_tag == "zh" {
        "当前运行态语言为中文。若用户当前在用中文交流，请优先用中文更新任务的 subject、description 和 activeForm；只有代码、命令、路径和必须保留的专有名词保持英文。"
    } else {
        "The current runtime language is English. Prefer English for task subject, description, and activeForm unless the user is clearly operating in another language."
    };

    format!(
        r#"{lang_header}

Use this tool to update a task in the task list.

## When to Use This Tool

**Mark tasks as resolved:**
- When you have completed the work described in a task
- When a task is no longer needed or has been superseded
- IMPORTANT: Always mark your assigned tasks as resolved when you finish them
- IMPORTANT: Update task state as you work so the checklist reflects progress in real time; do not wait until your final answer
- After resolving, call TaskList to find your next task

- ONLY mark a task as completed when you have FULLY accomplished it
- If you encounter errors, blockers, or cannot finish, keep the task as in_progress
- When blocked, create a new task describing what needs to be resolved
- Never mark a task as completed if:
  - Tests are failing
  - Implementation is partial
  - You encountered unresolved errors
  - You couldn't find necessary files or dependencies

**Delete tasks:**
- When a task is no longer relevant or was created in error
- Setting status to `deleted` permanently removes the task

**Update task details:**
- When requirements change or become clearer
- When establishing dependencies between tasks

## Fields You Can Update

- **status**: The task status (see Status Workflow below)
- **subject**: Change the task title (imperative form, e.g., "Run tests")
- **description**: Change the task description
- **activeForm**: Present continuous form shown in spinner when in_progress (e.g., "Running tests")
- **owner**: Change the task owner (agent name)
- **metadata**: Merge metadata keys into the task (set a key to null to delete it)
- **addBlocks**: Mark tasks that cannot start until this one completes
- **addBlockedBy**: Mark tasks that must complete before this one can start

## Status Workflow

Status progresses: `pending` → `in_progress` → `completed`

Use `deleted` to permanently remove a task.

## Staleness

Make sure to read a task's latest state using `TaskGet` before updating it.

## Examples

Mark task as in progress when starting work:
```json
{{"taskId": "1", "status": "in_progress"}}
```

Mark task as completed after finishing work:
```json
{{"taskId": "1", "status": "completed"}}
```

Delete a task:
```json
{{"taskId": "1", "status": "deleted"}}
```

Claim a task by setting owner:
```json
{{"taskId": "1", "owner": "my-name"}}
```

Set up task dependencies:
```json
{{"taskId": "2", "addBlockedBy": ["1"]}}
```
"#,
        lang_header = lang_header,
    )
}
