#!/usr/bin/env python3
"""M4.1 - current Rust auto-compact context smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M4.1",
        script_name=Path(__file__).name,
        steps=[
            Step(
                "auto_compact_updates_tracking",
                cargo_test(
                    "-p",
                    "mossen-agent",
                    "--lib",
                    "context::tests::auto_compact_returns_compacted_messages_and_updates_tracking",
                ),
                ("test result: ok.", "1 passed;"),
            ),
            Step(
                "auto_compact_hook_context",
                cargo_test(
                    "-p",
                    "mossen-agent",
                    "--lib",
                    "context::tests::auto_compact_forwards_hook_context_with_auto_trigger",
                ),
                ("test result: ok.", "1 passed;"),
            ),
        ],
        checks=[
            source_check(
                "auto_compact_threshold_and_boundary_source",
                "crates/mossen-agent/src/context/mod.rs",
                [
                    "pub fn auto_compact_threshold",
                    "pub async fn auto_compact_if_needed",
                    "build_auto_compact_boundary_message",
                    "hook_context",
                ],
            )
        ],
        design_note=(
            "M4.1 validates current Rust auto-compact mechanics directly: "
            "threshold calculation triggers compaction, state tracking is updated, "
            "and auto-triggered compaction forwards hook context."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
