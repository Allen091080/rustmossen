#!/usr/bin/env python3
"""W183 - final summary preserves file-change and diff context separately."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    render = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "self.current_file_change_widget.as_ref()",
        '"fileChangeSummary": file_change.cloned().unwrap_or(Value::Null)',
        '"diffSummary": diff.cloned().unwrap_or(Value::Null)',
        '"files: {summary}"',
        '"diff: {summary}"',
        '["widget"]["fileChangeSummary"]["summary"]',
    ]:
        require(render, token, "final summary change context", failures)

    require(
        structured,
        "terminal_final_summary_file_change_context",
        "status metadata",
        failures,
    )
    require(
        run_all,
        "wave_w183_terminal_final_summary_change_context_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render final summary change context",
        "phase note",
        failures,
    )

    if failures:
        print("=== W183 terminal final summary change context smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w183_terminal_final_summary_change_context_smoke: ok")


if __name__ == "__main__":
    main()
