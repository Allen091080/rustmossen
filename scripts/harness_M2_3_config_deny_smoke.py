#!/usr/bin/env python3
"""
M2.3 - current Rust configured deny-rule smoke.

The current Rust runtime represents session/config permission rules through the
same normalized allow/deny environment contract. This gate proves configured
deny takes precedence over allow and produces a deny tool-result path.
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
            name="deny_rule_takes_precedence_over_allow",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "session_permission_rules_deny_precedes_allow",
            ),
            timeout_secs=180,
        ),
        Step(
            name="path_prefix_deny_matches_file_tool_inputs",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "session_permission_rules_match_file_path_prefixes",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "deny_rules_are_checked_before_allow_rules",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "let deny_rules = permission_rule_env_lines(PERMISSION_DENY_RULES_ENV)",
                "let allow_rules = permission_rule_env_lines(PERMISSION_ALLOW_RULES_ENV)",
                "session_permission_rules_deny_precedes_allow",
                "Tool call denied by session permission rule.",
            ],
        ),
        source_check(
            "permission_rules_match_command_and_path_candidates",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "permission_rule_candidates(tool_name, input)",
                "format!(\"{tool_name} {value}\")",
                "permission_rule_path_prefix_matches(rule, candidate)",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M2.3_config_deny_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M2.3 validates current Rust deny-rule precedence and path-prefix "
            "matching without retired settings-wrapper probes."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
