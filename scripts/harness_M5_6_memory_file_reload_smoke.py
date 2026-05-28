#!/usr/bin/env python3
"""M5.6 — project memory file edits are reloaded on the next prompt build."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M5.6",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="project_memory_reload_reads_updated_file",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "system_prompt::tests::project_memory_reload_reads_updated_file",
                ),
            )
        ],
        checks=[
            source_check(
                "gather_memory_text_reads_files_each_call",
                "crates/mossen-cli/src/system_prompt.rs",
                [
                    "tokio::fs::read_to_string(&p).await",
                    "tokio::fs::read_to_string(&nested).await",
                    "pub async fn gather_memory_text",
                ],
            )
        ],
        design_note=(
            "M5.6 validates there is no stale project-memory cache between prompt "
            "builds: rewriting MOSSEN.md changes the next gathered memory block."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
