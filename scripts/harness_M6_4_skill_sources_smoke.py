#!/usr/bin/env python3
"""M6.4 — bundled, user, and project skill sources are covered."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M6.4",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="startup_loads_user_and_project_skill_sources",
                command=cargo_test(
                    "-p",
                    "mossen-skills",
                    "--lib",
                    "dynamic::tests::startup_loads_user_and_project_skill_sources",
                ),
            ),
            Step(
                name="slash_skills_reports_bundled_source",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "structured_io::tests::slash_command_skills_lists_available_inventory_redacted",
                ),
            ),
        ],
        checks=[
            source_check(
                "bundled_registration_current_rust",
                "crates/mossen-skills/src/bundled.rs",
                [
                    "pub fn init_bundled_skills()",
                    "register_simplify_skill();",
                    "register_loop_skill();",
                ],
            ),
            source_check(
                "startup_source_order_user_then_project",
                "crates/mossen-skills/src/dynamic.rs",
                [
                    "PromptCommandSource::UserSettings",
                    "PromptCommandSource::ProjectSettings",
                    "dirs.extend(",
                ],
            ),
        ],
        design_note=(
            "M6.4 validates source coverage: bundled skills are registered, "
            "user-level skills load from config home, and project skills load from .mossen/skills."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
