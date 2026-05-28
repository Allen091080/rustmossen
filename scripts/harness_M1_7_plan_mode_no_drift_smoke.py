#!/usr/bin/env python3
"""
M1.7 - current Rust plan-mode permission smoke.

The old script expected a real model to attempt EnterPlanMode. The current
deterministic contract is the permission-mode decision matrix:
  - plan mode blocks mutating tools;
  - ExitPlanMode remains allowed so the user can leave plan mode;
  - mode aliases parse to the expected internal mode.
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
            name="plan_mode_blocks_mutating_tools",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "plan_mode_blocks_mutating_tools_but_allows_plan_release",
            ),
            timeout_secs=180,
        ),
        Step(
            name="permission_mode_aliases_parse",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "permission_mode_parse_accepts_ui_and_sdk_spellings",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "plan_mode_decision_blocks_edit_write_but_allows_exit",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "PermissionMode::Plan",
                "if tool_name == \"ExitPlanMode\"",
                "Plan mode allows read-only exploration only",
                "plan_mode_blocks_mutating_tools_but_allows_plan_release",
            ],
        ),
        source_check(
            "plan_tools_are_registered_when_enabled",
            "crates/mossen-tools/src/lib.rs",
            [
                "plan_mode_tools_enabled()",
                "tools.push(Box::new(enter_plan_mode::PlanGate))",
                "tools.push(Box::new(exit_plan_mode::PlanRelease))",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M1.7_plan_mode_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M1.7 validates current Rust plan-mode permission behavior without "
            "a real model deciding whether to call EnterPlanMode."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
