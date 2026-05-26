#!/usr/bin/env python3
"""M6.6 — one broken skill file does not break other skills."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M6.6",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="bad_entry_skipped_good_skill_kept",
                command=cargo_test(
                    "-p",
                    "mossen-skills",
                    "--lib",
                    "dynamic::tests::load_skills_from_dir_skips_bad_entry_and_keeps_good_skill",
                ),
            )
        ],
        checks=[
            source_check(
                "loader_skips_unreadable_entries",
                "crates/mossen-skills/src/loader.rs",
                [
                    "tokio::fs::read_to_string(&skill_file_path).await",
                    "continue;",
                    "load_skills_from_dir",
                ],
            )
        ],
        design_note=(
            "M6.6 validates fail-soft loading: an unreadable/bad SKILL.md entry "
            "is skipped while a valid sibling skill remains discoverable and invocable."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
