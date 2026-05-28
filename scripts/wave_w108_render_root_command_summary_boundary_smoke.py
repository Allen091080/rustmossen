#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
MODEL = ROOT / "crates/mossen-tui/src/render_model.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def reject(text: str, needle: str, label: str) -> None:
    if needle in text:
        raise SystemExit(f"forbidden {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    model = MODEL.read_text()
    run_all = RUN_ALL.read_text()

    require(
        app,
        "let commands = command_summaries_from_messages(messages);",
        "root final summary semantic command source",
    )
    for forbidden in [
        "fn collect_command_summaries",
        "fn tool_json_payload",
        "fn command_from_payload",
        "fn cwd_from_payload",
        "fn exit_code_from_payload",
        "fn duration_ms_from_payload",
        "fn command_from_tool_preview",
        "fn json_search_preview",
        "fn non_empty_search_line",
        "serde_json::from_str::<serde_json::Value>(content)",
    ]:
        reject(app, forbidden, "root tool JSON compatibility parser")

    require(
        model,
        "pub fn command_summaries_from_messages",
        "semantic command summary helper",
    )
    require(
        model,
        "command_summaries_are_derived_from_semantic_transcript_runs",
        "semantic command summary regression test",
    )
    require(
        model,
        "CommandRunStatus::Succeeded",
        "command status mapping",
    )
    require(
        run_all,
        "wave_w108_render_root_command_summary_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w108_render_root_command_summary_boundary_smoke: ok")


if __name__ == "__main__":
    main()
