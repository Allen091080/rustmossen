#!/usr/bin/env python3
"""
M2.2 - current Rust allow/execute smoke.

This replaces the real-LLM Write e2e with deterministic gates proving that an
allowed tool path both passes permission rules and performs the filesystem
write.
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
            name="write_tool_creates_file_from_context",
            command=cargo_test(
                "-p",
                "mossen-tools",
                "write_relative_path_resolves_against_tool_context_cwd",
            ),
            timeout_secs=180,
        ),
        Step(
            name="session_allow_rule_matches_tool_input",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "session_permission_rules_allow_matching_tool_inputs",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "write_tool_registered_and_writes_file",
            "crates/mossen-tools/src/file_write.rs",
            [
                "fn name(&self) -> &str",
                "\"Write\"",
                "tmp.persist(path)",
                "write_relative_path_resolves_against_tool_context_cwd",
            ],
        ),
        source_check(
            "dialogue_allow_rules_short_circuit_permission_gate",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "session_permission_rules_allow_matching_tool_inputs",
                "PermissionDecision::Allow",
                "\"session_permission_rules\"",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M2.2_allow_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M2.2 validates current Rust allow-rule matching and Write tool "
            "execution without a real model."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
