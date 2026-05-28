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
        ("task_started_transcript_facts(", "root task-start progress facts call"),
        ("task_completed_transcript_facts(", "root task-completed progress facts call"),
        (
            "exceptional_stop_reason_transcript_message(",
            "root exceptional stop-reason facts call",
        ),
    ]:
        require(app, needle, label)

    for forbidden in [
        '"│ {} started ({})"',
        '"│ {} completed ({})"',
        'format!("{}:result", task_record_id(&tid))',
        'format!("(stop: {})"',
    ]:
        reject(app, forbidden, "root progress transcript formatting")

    for needle, label in [
        ("pub struct TaskProgressTranscriptFacts", "task progress transcript model"),
        ("pub fn task_started_transcript_facts", "task-start transcript helper"),
        ("pub fn task_completed_transcript_facts", "task-completed transcript helper"),
        (
            "pub fn exceptional_stop_reason_transcript_message",
            "exceptional stop-reason transcript helper",
        ),
        (
            "progress_transcript_facts_format_task_and_stop_rows",
            "progress transcript regression",
        ),
    ]:
        require(lifecycle, needle, label)

    require(
        run_all,
        "wave_w116_render_progress_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w116_render_progress_boundary_smoke: ok")


if __name__ == "__main__":
    main()
