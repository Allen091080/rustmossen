#!/usr/bin/env python3
"""W176 - terminal-render background Bash tasks survive and stay controllable."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    bash = (ROOT / "crates/mossen-tools/src/bash.rs").read_text()
    task_store = (ROOT / "crates/mossen-tools/src/task_store.rs").read_text()
    task_output = (ROOT / "crates/mossen-tools/src/task_output.rs").read_text()
    task_stop = (ROOT / "crates/mossen-tools/src/task_stop.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "create_background_shell_task",
        "register_background_shell_process",
        "finish_background_shell_task",
        "backgroundTaskId",
        "bash_background_task_returns_id_and_records_output",
        "bash_background_task_stop_kills_process_group_children",
    ]:
        require(bash, token, "bash background lifecycle", failures)

    for token in [
        "BackgroundShellProcess",
        "stop_background_task",
        "terminate_background_process",
        "is_task_ready_status",
        "timedOut",
    ]:
        require(task_store, token, "task store background shell process state", failures)

    require(
        task_output,
        "tokio::time::sleep(std::time::Duration::from_millis(50)).await",
        "TaskOutput blocking poll",
        failures,
    )
    require(
        task_output,
        "crate::task_store::is_task_ready_status(&r.status)",
        "TaskOutput terminal status mapping",
        failures,
    )
    require(
        task_output,
        "task_output_blocks_until_background_task_is_ready",
        "TaskOutput blocking test",
        failures,
    )
    require(
        task_stop,
        "crate::task_store::stop_background_task(&id)",
        "TaskStop real stop bridge",
        failures,
    )
    require(
        structured,
        "terminal_background_bash_task_lifecycle",
        "status metadata",
        failures,
    )
    require(
        run_all,
        "wave_w176_terminal_background_bash_task_lifecycle_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render background Bash task lifecycle",
        "phase note",
        failures,
    )

    if failures:
        print("=== W176 terminal background Bash task lifecycle smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w176_terminal_background_bash_task_lifecycle_smoke: ok")


if __name__ == "__main__":
    main()
