#!/usr/bin/env python3
"""
M2.1 - current Rust deny/dangerous-command smoke.

The retired smoke asked a real model to run `rm -rf` through an old launcher.
This gate validates the deterministic Rust layers that block dangerous or denied
tool calls: bash path validation, destructive warning detection, and session
permission-rule denial before allow.
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
            name="bash_path_validation_blocks_dangerous_removal",
            command=cargo_test(
                "-p",
                "mossen-tools",
                "test_check_dangerous_removal",
            ),
            timeout_secs=180,
        ),
        Step(
            name="bash_destructive_warning_marks_rm_rf",
            command=cargo_test("-p", "mossen-tools", "test_rm_rf"),
            timeout_secs=180,
        ),
        Step(
            name="session_deny_rule_precedes_allow",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "session_permission_rules_deny_precedes_allow",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "bash_path_validation_has_dangerous_removal_gate",
            "crates/mossen-tools/src/bash_tool/path_validation.rs",
            [
                "pub fn check_dangerous_removal_paths(command: &str) -> Option<String>",
                "rm -rf",
                "test_check_dangerous_removal",
            ],
        ),
        source_check(
            "dialogue_deny_short_circuits_to_error_tool_result",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "session_permission_rule_decision(tool_name, &input)",
                "PermissionDecision::Deny",
                "Tool call denied by session permission rule.",
                "is_error: Some(true)",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M2.1_deny_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M2.1 validates current Rust dangerous-command and deny-rule "
            "blocking without invoking a real model."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
