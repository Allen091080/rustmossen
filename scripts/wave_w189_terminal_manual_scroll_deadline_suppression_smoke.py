#!/usr/bin/env python3
"""W189 - manual scroll hold suppresses stale flush deadlines."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    renderer = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "draw_runtime_suppresses_stale_deadline_while_manual_scroll_holds_pending_draw",
        "self.next_flush_due_ms = None;",
        "skip_reason: Some(\"manual_scroll_preserved\".to_string())",
        "next_flush_due_ms: None",
        "held past stale deadline",
    ]:
        require(renderer, token, "manual-scroll stale deadline suppression", failures)

    for token in [
        "draw_runtime_manual_scroll_deadline_suppression",
        "draw_runtime_manual_scroll_no_busy_retry",
        "manual_scroll_deadline_suppression",
        "manual_scroll_no_busy_retry",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w189_terminal_manual_scroll_deadline_suppression_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render manual-scroll stale deadline suppression",
        "phase note",
        failures,
    )

    if failures:
        print("=== W189 terminal manual-scroll deadline suppression smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w189_terminal_manual_scroll_deadline_suppression_smoke: ok")


if __name__ == "__main__":
    main()
