#!/usr/bin/env python3
"""
M1.2 - current Rust Bash tool smoke.

This gate proves the Rust Bash tool executes a command and returns stdout, and
that the dialogue loop can surface tool summaries.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    steps = [
        Step(
            name="bash_tool_executes_command_and_returns_stdout",
            command=cargo_test(
                "-p",
                "mossen-tools",
                "bash_echo_command_returns_stdout_marker",
            ),
            timeout_secs=180,
        ),
        Step(
            name="dialogue_tool_result_reaches_model",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "harness_executes_glob_and_continues_after_openai_compatible_tool_result",
            ),
            timeout_secs=240,
        ),
    ]
    checks = [
        source_check(
            "bash_tool_registered_and_executes_shell",
            "crates/mossen-tools/src/bash.rs",
            [
                "fn name(&self) -> &str",
                "\"Bash\"",
                "tokio::process::Command",
                "bash_echo_command_returns_stdout_marker",
            ],
        ),
        source_check(
            "dialogue_records_tool_result_error_state",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "result.is_error",
                "ContentBlock::ToolResult",
                "SdkMessage::ToolUseSummary",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M1.2_bash_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M1.2 validates current Rust Bash execution and tool-result "
            "plumbing without using a real model or retired launcher."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
