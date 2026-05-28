#!/usr/bin/env python3
"""
M1.5 - current Rust tool failure recovery smoke.

This validates that a failing tool produces an error path and the dialogue loop
continues to a follow-up model request instead of going idle.
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
            name="dialogue_continues_after_failing_tool",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "dialogue_executes_post_tool_use_failure_settings_hooks",
            ),
            timeout_secs=240,
        ),
    ]
    checks = [
        source_check(
            "tool_failure_emits_error_result_and_continues",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "if result.is_error",
                "execute_post_tool_use_failure_hooks_for_error",
                "tool_results.push(result_block)",
                "tool-failure-hook-session",
                "assert_eq!(requests.len(), 2)",
            ],
        ),
        source_check(
            "failing_harness_tool_returns_error",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "struct HarnessFailingGlobTool",
                "is_error: true",
                "glob failed from harness",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M1.5_tool_failure_recovery_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M1.5 validates current Rust tool failure recovery and continuation "
            "with a mock provider."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
