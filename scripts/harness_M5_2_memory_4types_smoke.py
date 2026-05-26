#!/usr/bin/env python3
"""M5.2 — current Rust memory frontmatter type taxonomy smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M5.2",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="scan_memory_files_parses_all_frontmatter_types",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "memdir::tests::scan_memory_files_parses_all_frontmatter_types",
                ),
            )
        ],
        checks=[
            source_check(
                "memory_types_are_current_rust_taxonomy",
                "crates/mossen-cli/src/memdir.rs",
                [
                    'pub const MEMORY_TYPES: &[&str] = &["user", "feedback", "project", "reference"];',
                    "pub fn parse_memory_type",
                    "fn parse_frontmatter_fields",
                ],
            )
        ],
        design_note=(
            "M5.2 validates the Rust scanner parses all four memory frontmatter "
            "types and excludes MEMORY.md from the per-memory manifest."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
