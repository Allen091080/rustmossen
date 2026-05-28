#!/usr/bin/env python3
"""
M1.3 - current Rust Edit tool smoke.

The retired version required a real model to choose Edit. This gate validates
the deterministic Rust behavior directly: Edit resolves paths from the tool
context, performs the file replacement, and the dialogue layer records tool
results for the model loop.
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
            name="edit_tool_replaces_file_content",
            command=cargo_test(
                "-p",
                "mossen-tools",
                "edit_relative_path_resolves_against_tool_context_cwd",
            ),
            timeout_secs=180,
        ),
        Step(
            name="permission_mode_accept_edits_allows_edit_tools_only",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "accept_edits_only_short_circuits_edit_tools",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "edit_tool_registered_and_uses_atomic_replacement",
            "crates/mossen-tools/src/file_edit.rs",
            [
                "fn name(&self) -> &str",
                "\"Edit\"",
                "fn atomic_write(path: &str, content: &str)",
                "edit_relative_path_resolves_against_tool_context_cwd",
            ],
        ),
        source_check(
            "dialogue_permission_flow_handles_edit_tools",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "matches!(tool_name, \"Edit\" | \"Write\" | \"NotebookEdit\")",
                "PermissionMode::AcceptEdits",
                "accept_edits_only_short_circuits_edit_tools",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M1.3_edit_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M1.3 validates current Rust Edit execution and edit permission "
            "routing without relying on real LLM behavior."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
