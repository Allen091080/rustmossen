#!/usr/bin/env python3
"""M6.1 — current Rust skill inventory is non-empty and user skills reach prompts."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M6.1",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="oneshot_system_prompt_includes_user_config_skill",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "repl::terminal_render_frontend_event_tests::oneshot_system_prompt_includes_user_config_skill",
                ),
            ),
            Step(
                name="slash_skills_lists_available_inventory",
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
                "startup_loads_user_level_skills",
                "crates/mossen-skills/src/dynamic.rs",
                [
                    "pub async fn load_startup_skill_directories",
                    "get_mossen_config_home_dir().join(\"skills\")",
                    "PromptCommandSource::UserSettings",
                ],
            ),
            source_check(
                "skills_slash_reports_available_inventory",
                "crates/mossen-cli/src/structured_io.rs",
                [
                    "\"availableCount\": available.len()",
                    "\"available\": available",
                    "\"rawSkillRootsIncluded\": false",
                ],
            ),
        ],
        design_note=(
            "M6.1 validates current Rust behavior: bundled skills plus user "
            "$MOSSEN_CONFIG_DIR/skills entries are visible to the model-facing "
            "skill inventory, and /skills reports available skills without leaking bodies or paths."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
