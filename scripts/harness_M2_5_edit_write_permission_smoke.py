#!/usr/bin/env python3
"""
M2.5 - current Rust Edit/Write permission smoke.

This replaces real-model Edit attempts with deterministic gates proving:
  - Edit actually mutates files when allowed;
  - allow rules match tool inputs;
  - deny rules take precedence over allow rules;
  - path-prefix rules apply to file tools such as Write/Edit.
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
            name="edit_tool_applies_replacement",
            command=cargo_test(
                "-p",
                "mossen-tools",
                "edit_relative_path_resolves_against_tool_context_cwd",
            ),
            timeout_secs=180,
        ),
        Step(
            name="allow_rule_matches_tool_input",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "session_permission_rules_allow_matching_tool_inputs",
            ),
            timeout_secs=180,
        ),
        Step(
            name="deny_rule_precedes_allow_for_edit_write_class",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "session_permission_rules_deny_precedes_allow",
            ),
            timeout_secs=180,
        ),
        Step(
            name="path_prefix_deny_matches_write_input",
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
            "edit_and_write_are_permission_tools",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "matches!(tool_name, \"Edit\" | \"Write\" | \"NotebookEdit\")",
                "session_permission_rule_decision(tool_name, &input)",
                "permission_rule_candidates(tool_name, input)",
            ],
        ),
        source_check(
            "edit_tool_uses_context_path_and_atomic_write",
            "crates/mossen-tools/src/file_edit.rs",
            [
                "\"Edit\"",
                "PathBuf::from(cwd).join(path)",
                "fn atomic_write(path: &str, content: &str)",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M2.5_edit_write_permission_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M2.5 validates current Rust Edit/Write permission decisions and "
            "Edit execution without relying on real LLM tool selection."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
