#!/usr/bin/env python3
"""M6.2 — Skill tool invokes a loaded Rust dynamic skill."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M6.2",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="skill_tool_executes_loaded_dynamic_skill",
                command=cargo_test(
                    "-p",
                    "mossen-tools",
                    "--lib",
                    "skill::tests::skill_tool_executes_loaded_dynamic_skill",
                ),
            )
        ],
        checks=[
            source_check(
                "skill_tool_uses_dynamic_and_bundled_registries",
                "crates/mossen-tools/src/skill.rs",
                [
                    "let mut crafts = mossen_skills::get_dynamic_skills();",
                    "crafts.extend(mossen_skills::get_bundled_crafts());",
                    "mossen_skills::execute_craft(",
                    "mossen_skills::format_invoked_skill_prompt(",
                ],
            )
        ],
        design_note=(
            "M6.2 validates the current Skill tool path: loaded dynamic skill -> "
            "CraftInvoker -> execute_craft -> structured tool output containing rendered SKILL.md body."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
