#!/usr/bin/env python3
"""W190 - fail-close synchronized terminal updates on draw write errors."""

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
        "terminal_draw_queue_or_fail_closed",
        "terminal_draw_fail_close_synchronized_update",
        "terminal::EndSynchronizedUpdate",
        "draw_executor_fail_closes_synchronized_update_on_write_error",
        "forced terminal write failure",
    ]:
        require(renderer, token, "sync-update fail-closed draw executor", failures)

    for token in [
        "terminal_synchronized_update_fail_closed",
        "draw_executor_error_fail_closed",
        "synchronized_update_fail_closed",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w190_terminal_sync_update_fail_closed_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render synchronized update fail-closed cleanup",
        "phase note",
        failures,
    )

    if failures:
        print("=== W190 terminal sync-update fail-closed smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w190_terminal_sync_update_fail_closed_smoke: ok")


if __name__ == "__main__":
    main()
