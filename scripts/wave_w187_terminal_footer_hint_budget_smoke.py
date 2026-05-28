#!/usr/bin/env python3
"""W187 - terminal footer uses bounded visible hints with overflow metadata."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    render_events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "STREAM_JSON_RENDER_FOOTER_VISIBLE_HINT_MAX",
        "terminal_footer_visible_hints",
        "terminal_footer_hint_overflow_count",
        "footerHintMax",
        "footerHintsBounded",
        "footerHints",
        "footerHintOverflowCount",
        "fullHints",
        "footer_hints.join",
        "terminal_footer_bounds_visible_hints_and_reports_overflow",
    ]:
        require(render_events, token, "footer hint budget", failures)

    for token in [
        "terminal_footer_hint_budget",
        "terminal_footer_hint_overflow_count",
        "terminal_footer_full_hints_snapshot",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w187_terminal_footer_hint_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render footer hint budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W187 terminal footer hint budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w187_terminal_footer_hint_budget_smoke: ok")


if __name__ == "__main__":
    main()
