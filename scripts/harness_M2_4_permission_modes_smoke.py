#!/usr/bin/env python3
"""
M2.4 - current Rust permission-mode smoke.

This gate covers the mode matrix directly in Rust:
  - aliases parse to the intended mode;
  - plan blocks mutating tools but allows ExitPlanMode;
  - acceptEdits auto-allows edit tools only;
  - bypassPermissions allows and dontAsk denies non-interactively.
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
            name="permission_mode_aliases_parse",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "permission_mode_parse_accepts_ui_and_sdk_spellings",
            ),
            timeout_secs=180,
        ),
        Step(
            name="plan_mode_blocks_mutating_tools",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "plan_mode_blocks_mutating_tools_but_allows_plan_release",
            ),
            timeout_secs=180,
        ),
        Step(
            name="accept_edits_only_allows_edit_tools",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "accept_edits_only_short_circuits_edit_tools",
            ),
            timeout_secs=180,
        ),
        Step(
            name="bypass_and_dont_ask_are_non_interactive",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "bypass_and_dont_ask_modes_are_non_interactive",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "dialogue_permission_mode_decision_matrix",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "PermissionMode::Default => None",
                "PermissionMode::AcceptEdits",
                "PermissionMode::BypassPermissions | PermissionMode::Auto | PermissionMode::Yolo",
                "PermissionMode::Plan",
                "PermissionMode::DontAsk",
            ],
        ),
        source_check(
            "stream_json_permissions_command_sets_session_mode",
            "crates/mossen-cli/src/structured_io.rs",
            [
                "parse_permission_mode_arg",
                "std::env::set_var(PERMISSION_MODE_ENV",
                "permission_mode_picker_payload(mode)",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M2.4_permission_modes_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M2.4 validates the current Rust permission-mode matrix and "
            "stream-json session mode wiring."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
