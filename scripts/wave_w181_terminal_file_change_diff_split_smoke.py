#!/usr/bin/env python3
"""W181 - terminal renderer keeps file-change summary separate from diff."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    render = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    terminal_renderer = (
        ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs"
    ).read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "current_file_change_widget: Option<Value>",
        '"fileChanges": {',
        '"fileChangeRegionId": if file_change_active { "file_changes" } else { "" }',
        '"replace_file_summary"',
        "fn terminal_file_change_lines",
        "terminal_frame_keeps_file_change_summary_separate_from_diff",
        "file_change_index < diff_index",
    ]:
        require(render, token, "file-change/diff split", failures)

    require(terminal_renderer, '"file_changes" => {', "file-change semantic style", failures)
    for token in [
        "terminal_file_change_summary_region",
        "independent_file_change_region",
        "terminal_file_change_diff_separation",
    ]:
        require(structured, token, "status metadata", failures)
    require(
        run_all,
        "wave_w181_terminal_file_change_diff_split_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render file-change and diff split",
        "phase note",
        failures,
    )

    if failures:
        print("=== W181 terminal file-change/diff split smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w181_terminal_file_change_diff_split_smoke: ok")


if __name__ == "__main__":
    main()
