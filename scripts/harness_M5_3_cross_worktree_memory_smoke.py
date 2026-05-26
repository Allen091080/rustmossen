#!/usr/bin/env python3
"""M5.3 — user-global memory is shared across project cwd roots."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M5.3",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="global_user_memory_is_shared_across_cwds",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "system_prompt::tests::global_user_memory_is_shared_across_cwds",
                ),
            )
        ],
        checks=[
            source_check(
                "system_prompt_reads_user_global_mossen_md",
                "crates/mossen-cli/src/system_prompt.rs",
                [
                    'home.join(".mossen").join("MOSSEN.md")',
                    "user's private global instructions for all projects",
                    "pub async fn gather_memory_text",
                ],
            )
        ],
        design_note=(
            "M5.3 validates that ~/.mossen/MOSSEN.md is injected for different "
            "cwd roots, proving user-global memory is not scoped to one worktree."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
