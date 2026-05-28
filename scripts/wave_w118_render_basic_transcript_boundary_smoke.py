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
    app_runtime = app.split("#[cfg(test)]", 1)[0]
    lifecycle = LIFECYCLE.read_text()
    run_all = RUN_ALL.read_text()

    for needle, label in [
        ("system_transcript_message(", "root system transcript helper call"),
        ("user_transcript_message(", "root user transcript helper call"),
        (
            "command_output_transcript_message(",
            "root command-output transcript helper call",
        ),
        (
            "skill_invocation_transcript_message(",
            "root skill invocation transcript helper call",
        ),
        (
            "cancelled_transcript_message(",
            "root cancelled transcript helper call",
        ),
        (
            "final_summary_transcript_message(",
            "root final-summary transcript helper call",
        ),
        (
            "unknown_command_transcript_message(",
            "root unknown-command transcript helper call",
        ),
    ]:
        require(app_runtime, needle, label)

    for forbidden in [
        "message_type: MessageType::System",
        "message_type: MessageType::User",
        "message_type: MessageType::CommandOutput",
        "message_type: MessageType::SkillInvocation",
        'format!("/{}\\n{}"',
        'format!("Unknown command: /{}"',
        'format!("/{}  ({})\\nresolving template:\\n{}"',
        '"↯ Cancelled".to_string()',
    ]:
        reject(app_runtime, forbidden, "root basic transcript MessageData formatting")

    for needle, label in [
        ("pub fn system_transcript_message", "system transcript helper"),
        ("pub fn user_transcript_message", "user transcript helper"),
        (
            "pub fn command_output_transcript_message",
            "command-output transcript helper",
        ),
        (
            "pub fn skill_invocation_transcript_message",
            "skill invocation transcript helper",
        ),
        ("pub fn cancelled_transcript_message", "cancelled transcript helper"),
        (
            "pub fn final_summary_transcript_message",
            "final-summary transcript helper",
        ),
        (
            "pub fn unknown_command_transcript_message",
            "unknown-command transcript helper",
        ),
        (
            "basic_transcript_messages_format_user_system_command_skill_and_summary",
            "basic transcript regression",
        ),
    ]:
        require(lifecycle, needle, label)

    require(
        run_all,
        "wave_w118_render_basic_transcript_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w118_render_basic_transcript_boundary_smoke: ok")


if __name__ == "__main__":
    main()
