#!/usr/bin/env python3
"""W175 - terminal-render shell cancellation terminates process groups."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    bash = (ROOT / "crates/mossen-tools/src/bash.rs").read_text()
    tools_lib = (ROOT / "crates/mossen-tools/src/lib.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "ForegroundProcessGroupGuard",
        "configure_foreground_process_group",
        "command.process_group(0)",
        "terminate_process_group",
        "killpg(pgid, Signal::SIGTERM)",
        "killpg(pgid, Signal::SIGKILL)",
        "bash_timeout_kills_foreground_process_group_children",
    ]:
        require(bash, token, "bash foreground process-group termination", failures)

    require(
        tools_lib,
        "Box::new(bash::ShellExecutor)",
        "registered Bash tool path",
        failures,
    )
    require(
        structured,
        "terminal_shell_process_group_termination",
        "status metadata",
        failures,
    )
    require(
        run_all,
        "wave_w175_terminal_shell_process_group_cancel_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render shell process-group cancellation",
        "phase note",
        failures,
    )

    if failures:
        print("=== W175 terminal shell process-group cancel smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w175_terminal_shell_process_group_cancel_smoke: ok")


if __name__ == "__main__":
    main()
