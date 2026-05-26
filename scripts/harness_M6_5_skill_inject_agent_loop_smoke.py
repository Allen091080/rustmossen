#!/usr/bin/env python3
"""M6.5 — Skill output injects rendered body for the model follow-up."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M6.5",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="skill_tool_result_contains_rendered_body",
                command=cargo_test(
                    "-p",
                    "mossen-tools",
                    "--lib",
                    "skill::tests::skill_tool_result_contains_rendered_body_for_model_followup",
                ),
            )
        ],
        checks=[
            source_check(
                "skill_prompt_command_tags_and_body",
                "crates/mossen-skills/src/executor.rs",
                [
                    "pub fn format_invoked_skill_prompt",
                    "format_command_input_tags(skill_name, args)",
                    "format!(\"{tags}\\n\\n{body}\")",
                ],
            ),
            source_check(
                "skill_tool_returns_rendered_result",
                "crates/mossen-tools/src/skill.rs",
                [
                    "result: Some(result)",
                    "resultIncludesCommandTags",
                    "M6_5_FORCED_END_MARKER_xyz",
                ],
            ),
        ],
        design_note=(
            "M6.5 validates the deterministic Rust contract behind model behavior: "
            "Skill execution returns rendered SKILL.md body plus command tags, so "
            "the next model step receives the skill instructions."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
