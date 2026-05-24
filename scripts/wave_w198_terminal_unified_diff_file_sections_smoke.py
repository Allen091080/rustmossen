#!/usr/bin/env python3
"""W198 - terminal diff viewer groups unified diff previews by file."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "TerminalUnifiedDiffFileSection",
        "terminal_parse_unified_diff_file_sections",
        "terminal_unified_diff_git_header_path",
        "diffFileSections",
        "expandedDiffFileSections",
        "diffFileSectionPreviewAvailable",
        "terminal_diff_widget_expanded_groups_unified_diff_by_file",
    ]:
        require(events, token, "unified diff file-section renderer", failures)

    for token in [
        "terminal_unified_diff_file_sections",
        "terminal_diff_file_grouped_preview",
        "terminal_diff_per_file_hunk_preview",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w198_terminal_unified_diff_file_sections_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render unified diff file sections",
        "phase note",
        failures,
    )

    if failures:
        print("=== W198 terminal unified diff file sections smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w198_terminal_unified_diff_file_sections_smoke: ok")


if __name__ == "__main__":
    main()
