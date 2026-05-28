#!/usr/bin/env python3
"""W64 — current Rust /skills bundled visibility smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W64",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="slash_skills_lists_bundled_and_dynamic_inventory",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "structured_io::tests::slash_command_skills_lists_available_inventory_redacted",
                ),
            )
        ],
        checks=[
            source_check(
                "tui_skills_panel_reads_bundled_skills",
                "crates/mossen-tui/src/app.rs",
                [
                    "fn build_skills_panel_state",
                    "for craft in mossen_skills::get_bundled_crafts()",
                    "No skills discovered",
                ],
            ),
            source_check(
                "stream_json_skills_inventory_reads_bundled_skills",
                "crates/mossen-cli/src/structured_io.rs",
                [
                    "let mut available = mossen_skills::get_bundled_crafts();",
                    "\"source\": slash_skill_source_label(skill.loaded_from)",
                    "CommandLoadedFrom::Bundled => \"bundled\"",
                ],
            ),
            source_check(
                "bundled_skills_are_registered_as_bundled",
                "crates/mossen-skills/src/skill.rs",
                [
                    "loaded_from: Some(CommandLoadedFrom::Bundled)",
                    "loaded_from: CommandLoadedFrom::Bundled",
                    "pub fn get_bundled_crafts()",
                ],
            ),
        ],
        design_note=(
            "W64 validates that /skills can see bundled skills in the Rust TUI and "
            "stream-json surfaces instead of regressing to an empty inventory."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
