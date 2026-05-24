#!/usr/bin/env python3
"""W172 - terminal-render key release filter smoke."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "KeyEventKind",
        "key.kind == KeyEventKind::Release",
        "return None;",
        "ignores_key_release_events_to_prevent_duplicate_actions",
        "KeyEventKind::Repeat",
        "TerminalRenderFrontendEvent::ManualScrollStart",
    ]:
        require(repl, token, "terminal key release filter", failures)

    require(
        structured,
        "terminal_frontend_key_release_filter",
        "status metadata",
        failures,
    )
    require(
        run_all,
        "wave_w172_terminal_key_release_filter_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render key-release filter",
        "phase note",
        failures,
    )

    if failures:
        print("=== W172 terminal key-release filter smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w172_terminal_key_release_filter_smoke: ok")


if __name__ == "__main__":
    main()
