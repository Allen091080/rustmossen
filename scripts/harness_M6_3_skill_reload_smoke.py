#!/usr/bin/env python3
"""M6.3 — skill reload sees edited SKILL.md content."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M6.3",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="skill_reload_updates_existing_content",
                command=cargo_test(
                    "-p",
                    "mossen-skills",
                    "--lib",
                    "dynamic::tests::add_skill_directories_reload_updates_existing_skill_content",
                ),
            )
        ],
        checks=[
            source_check(
                "add_skill_directories_reloads_existing_names",
                "crates/mossen-skills/src/dynamic.rs",
                [
                    "pub async fn add_skill_directories_in_precedence_order",
                    "s.skills.insert(name.clone(), skill.clone())",
                    "add_skill_directories_reload_updates_existing_skill_content",
                ],
            )
        ],
        design_note=(
            "M6.3 validates that re-adding a skill directory refreshes an existing "
            "skill's markdown body instead of pinning stale content."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
