#!/usr/bin/env python3
"""M5.4 — project MOSSEN.md is loaded in a fresh session prompt."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M5.4",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="project_memory_is_loaded_for_fresh_window",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "system_prompt::tests::project_memory_is_loaded_for_fresh_window",
                ),
            )
        ],
        checks=[
            source_check(
                "system_prompt_reads_project_memory_files",
                "crates/mossen-cli/src/system_prompt.rs",
                [
                    'for filename in ["MOSSEN.md", "MOSSEN.local.md"]',
                    "Contents of {}:",
                    "crate::memdir::load_memory_prompt(cwd).await",
                ],
            )
        ],
        design_note=(
            "M5.4 validates that a new prompt for the project cwd loads "
            "MOSSEN.md without needing resume state."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
