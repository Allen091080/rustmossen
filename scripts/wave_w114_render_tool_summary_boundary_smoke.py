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

    require(
        app,
        "tool_summary_transcript_facts(",
        "root tool-summary semantic transcript facts call",
    )
    require(
        app,
        "self.messages.push(transcript_facts.message);",
        "root tool-summary message push from facts",
    )
    for forbidden in [
        "fn scoped_tool_record_id",
        "fn task_record_id",
        "fn tool_result_record_id",
        "let parent_id = tool_use_id",
    ]:
        reject(app, forbidden, "root tool-summary record-id helper")

    require(
        lifecycle,
        "pub struct ToolSummaryTranscriptFacts",
        "tool-summary transcript facts model",
    )
    require(
        lifecycle,
        "pub fn tool_summary_transcript_facts",
        "tool-summary transcript facts helper",
    )
    require(
        lifecycle,
        "pub fn scoped_tool_record_id",
        "scoped tool record id helper",
    )
    require(
        lifecycle,
        "tool_summary_transcript_facts_scope_parent_ids_and_message",
        "tool-summary transcript facts regression",
    )
    require(
        run_all,
        "wave_w114_render_tool_summary_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w114_render_tool_summary_boundary_smoke: ok")


if __name__ == "__main__":
    main()
