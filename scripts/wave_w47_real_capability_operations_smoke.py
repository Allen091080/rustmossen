#!/usr/bin/env python3
"""W47 — current Rust capability operation safety smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W47",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="slash_compact_preview_enqueues_dry_run_request",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "structured_io::tests::slash_command_compact_preview_enqueues_dry_run_request",
                ),
            ),
            Step(
                name="slash_compact_run_requires_confirm",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "structured_io::tests::slash_command_compact_run_requires_confirm",
                ),
            ),
            Step(
                name="slash_compact_run_confirm_enqueues_real_request",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "structured_io::tests::slash_command_compact_run_confirm_enqueues_real_request",
                ),
            ),
            Step(
                name="runtime_doctor_reports_memory_and_compact_checks",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "structured_io::tests::slash_command_doctor_reports_memory_and_compact_checks",
                ),
            ),
            Step(
                name="compact_control_request_enqueues_and_responds",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "structured_io::tests::compact_conversation_control_request_enqueues_and_responds",
                ),
            ),
            Step(
                name="compact_control_request_blocks_unsupported_mode",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "structured_io::tests::compact_conversation_control_request_blocks_unsupported_mode",
                ),
            ),
        ],
        checks=[
            source_check(
                "compact_operation_requires_explicit_confirmation",
                "crates/mossen-cli/src/structured_io.rs",
                [
                    "build_compact_slash_response(",
                    "enqueue_pending_compact_request(",
                    '"/compact run --confirm"',
                    '"will_mutate_history"',
                    "unsupported_slash_command_args: compact",
                ],
            ),
            source_check(
                "doctor_operation_is_readonly_runtime_summary",
                "crates/mossen-cli/src/structured_io.rs",
                [
                    "slash_command_doctor_reports_memory_and_compact_checks",
                    '"slashBridge"',
                    '"contentIncluded"',
                    '"autoCompactEnabled"',
                    '"pending"',
                ],
            ),
            source_check(
                "blocked_or_confirmed_slash_manifest_entries_present",
                "crates/mossen-agent/src/services/root/slash_command_capabilities.rs",
                [
                    '"slash.compact"',
                    '"slash.config"',
                    '"slash.doctor"',
                    '"slash.diff"',
                    '"slash.ide"',
                    "ArgsMode::Subcommand",
                    "SideEffect::ClearsConversation",
                ],
            ),
            source_check(
                "run_all_keeps_w47_registered",
                "scripts/run_all_smoke.sh",
                ["wave_w47_real_capability_operations_smoke.py"],
            ),
        ],
        design_note=(
            "W47 validates current Rust capability operation safety. It no longer "
            "checks removed SDK TS schemas; instead it executes structured_io tests "
            "for compact confirmation, compact control requests, and readonly doctor "
            "summaries, with source checks for current manifest safety contracts."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
