#!/usr/bin/env python3
"""M5.1 — current Rust auto-memory entrypoint and fresh scan smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M5.1",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="auto_memory_prompt_loads_entrypoint_content",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "memdir::tests::auto_memory_prompt_loads_entrypoint_content_from_override",
                ),
            ),
            Step(
                name="fresh_scan_reads_written_marker",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "memdir::tests::scan_memory_files_reads_written_marker_after_restart",
                ),
            ),
        ],
        checks=[
            source_check(
                "load_memory_prompt_uses_entrypoint_reader",
                "crates/mossen-cli/src/memdir.rs",
                [
                    "pub async fn load_memory_prompt",
                    "Some(build_memory_prompt(\"auto memory\", &auto_dir))",
                    "MOSSEN_COWORK_MEMORY_PATH_OVERRIDE",
                ],
            )
        ],
        design_note=(
            "M5.1 validates the current Rust auto-memory loader. The prompt path "
            "must include MEMORY.md entrypoint content, and a fresh scan must read "
            "a marker from disk without relying on stale wrapper-era Bun imports."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
