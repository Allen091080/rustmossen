#!/usr/bin/env python3
"""M4.3 - current Rust manual /compact bridge and safe-point smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M4.3",
        script_name=Path(__file__).name,
        steps=[
            Step(
                "compact_request_compacts_at_dialogue_safe_point",
                cargo_test(
                    "-p",
                    "mossen-agent",
                    "--lib",
                    "dialogue::tests::pending_compact_request_compacts_state_and_emits_boundary",
                ),
                ("test result: ok.", "1 passed;"),
            ),
            Step(
                "compact_request_dry_run_does_not_mutate",
                cargo_test(
                    "-p",
                    "mossen-agent",
                    "--lib",
                    "dialogue::tests::pending_compact_request_dry_run_does_not_mutate_or_emit_boundary",
                ),
                ("test result: ok.", "1 passed;"),
            ),
            Step(
                "compact_request_skipped_emits_status",
                cargo_test(
                    "-p",
                    "mossen-agent",
                    "--lib",
                    "dialogue::tests::pending_compact_request_skipped_emits_status_event",
                ),
                ("test result: ok.", "1 passed;"),
            ),
            Step(
                "slash_compact_preview_queues_dry_run",
                cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "structured_io::tests::slash_command_compact_preview_enqueues_dry_run_request",
                ),
                ("test result: ok.", "1 passed;"),
            ),
            Step(
                "slash_compact_run_confirm_queues_real_request",
                cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "structured_io::tests::slash_command_compact_run_confirm_enqueues_real_request",
                ),
                ("test result: ok.", "1 passed;"),
            ),
        ],
        checks=[
            source_check(
                "compact_bridge_safe_point_source",
                "crates/mossen-agent/src/dialogue.rs",
                [
                    "execute_pending_compact_request",
                    "dequeue_pending_compact_request",
                    "compact_request_status",
                    "compact_boundary",
                ],
            ),
            source_check(
                "slash_compact_payload_source",
                "crates/mossen-cli/src/structured_io.rs",
                [
                    "fn build_compact_slash_response",
                    '"safe_point": "dialogue_safe_point"',
                    '"expected_status_event": "compact_request_status"',
                    '"history_boundary"',
                    '"compact_boundary_on_safe_point"',
                ],
            ),
        ],
        design_note=(
            "M4.3 validates the current /compact path in two halves: "
            "stream-json slash commands enqueue preview/confirmed requests, "
            "and dialogue executes them only at the safe point with dry-run and "
            "skipped-state behavior covered."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
