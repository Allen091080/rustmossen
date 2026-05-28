#!/usr/bin/env python3
"""W51 — current Rust memory runtime diagnostics smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W51",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="slash_memory_runtime_diagnostics_are_redacted",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "structured_io::tests::slash_command_memory_response_reports_runtime_without_content",
                ),
            ),
            Step(
                name="slash_doctor_reports_memory_and_compact_checks",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "structured_io::tests::slash_command_doctor_reports_memory_and_compact_checks",
                ),
            ),
        ],
        checks=[
            source_check(
                "slash_memory_runtime_snapshot_current_rust",
                "crates/mossen-cli/src/structured_io.rs",
                [
                    "fn slash_memory_runtime_snapshot()",
                    "autoMemoryEnabled",
                    "extractModeActive",
                    "sessionMemory",
                    "autoCompactEnabled",
                    "contentIncluded",
                    "pathsRedacted",
                ],
            ),
            source_check(
                "doctor_embeds_memory_and_compact_without_external_checks",
                "crates/mossen-cli/src/structured_io.rs",
                [
                    '"externalChecksRun": false',
                    '"networkChecksRun": false',
                    '"installChecksRun": false',
                    '"memory": memory_runtime',
                    '"compact": {',
                    '"slashBridge": true',
                ],
            ),
            source_check(
                "run_all_registration",
                "scripts/run_all_smoke.sh",
                ["wave_w51_memory_diagnostics_smoke"],
            ),
        ],
        design_note=(
            "W51 validates the Rust /memory and /doctor diagnostics. The checks "
            "must be read-only, redacted, and must not depend on deleted TS files."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
