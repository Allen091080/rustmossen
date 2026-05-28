#!/usr/bin/env python3
"""W265 - terminal teardown releases manual-scroll hold before final flush."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    renderer = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "release_manual_scroll_for_terminal_teardown",
        "draw_runtime_releases_manual_scroll_hold_for_terminal_teardown",
        "teardown-visible final update",
    ]:
        require(renderer, token, "terminal draw runtime teardown release", failures)

    require(
        repl,
        "draw_runtime.release_manual_scroll_for_terminal_teardown()",
        "oneshot terminal final flush",
        failures,
    )

    for token in [
        "terminal_teardown_releases_manual_scroll_hold",
        "terminal_teardown_flushes_pending_draw_after_release",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w265_terminal_teardown_pending_flush_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render teardown pending flush",
        "phase note",
        failures,
    )

    if failures:
        print("=== W265 terminal teardown pending flush smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w265_terminal_teardown_pending_flush_smoke: ok")


if __name__ == "__main__":
    main()
