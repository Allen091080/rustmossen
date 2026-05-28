#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
LIFECYCLE = ROOT / "crates/mossen-tui/src/render_lifecycle.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def reject(text: str, needle: str, label: str) -> None:
    if needle in text:
        raise SystemExit(f"forbidden {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    lifecycle = LIFECYCLE.read_text()
    run_all = RUN_ALL.read_text()

    for needle, label in [
        ("assistant_transcript_message(", "root assistant transcript helper call"),
        (
            "pending_assistant_transcript_message(",
            "root pending assistant transcript helper call",
        ),
        (
            "task_assistant_transcript_facts(",
            "root task assistant transcript facts call",
        ),
        ("tool_use_transcript_facts(", "root tool-use transcript facts call"),
        (
            "finalize_pending_assistant_transcript_message(",
            "root pending assistant finalization call",
        ),
    ]:
        require(app, needle, label)

    for forbidden in [
        '"│ {}\\n{}"',
        '"agent  {}\\n{}"',
        'format!("({})"',
        "message_type: MessageType::Assistant,\n                            content: final_content.clone()",
        "message_type: MessageType::ToolUse,\n                        content: preview",
        "message_type: MessageType::Assistant,\n            content: String::new()",
    ]:
        reject(app, forbidden, "root assistant/tool transcript formatting")

    for needle, label in [
        ("pub struct AssistantTranscriptFacts", "assistant transcript facts model"),
        ("pub struct ToolUseTranscriptFacts", "tool-use transcript facts model"),
        (
            "pub enum PendingAssistantFinalization",
            "pending assistant finalization model",
        ),
        (
            "pub fn assistant_transcript_message",
            "assistant transcript message helper",
        ),
        (
            "pub fn pending_assistant_transcript_message",
            "pending assistant transcript helper",
        ),
        (
            "pub fn task_assistant_transcript_facts",
            "task assistant transcript helper",
        ),
        ("pub fn tool_use_transcript_facts", "tool-use transcript helper"),
        (
            "pub fn finalize_pending_assistant_transcript_message",
            "pending assistant finalization helper",
        ),
        (
            "assistant_transcript_facts_format_main_task_tool_and_pending_rows",
            "assistant transcript regression",
        ),
        (
            "pending_assistant_finalization_owns_empty_and_terminal_rules",
            "pending assistant finalization regression",
        ),
    ]:
        require(lifecycle, needle, label)

    require(
        run_all,
        "wave_w117_render_assistant_transcript_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w117_render_assistant_transcript_boundary_smoke: ok")


if __name__ == "__main__":
    main()
