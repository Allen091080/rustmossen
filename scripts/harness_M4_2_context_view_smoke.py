#!/usr/bin/env python3
"""M4.2 - current Rust /context slash snapshot smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M4.2",
        script_name=Path(__file__).name,
        steps=[
            Step(
                "slash_context_snapshot",
                cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "structured_io::tests::slash_command_context_reports_token_window_snapshot",
                ),
                ("test result: ok.", "1 passed;"),
            ),
        ],
        checks=[
            source_check(
                "context_payload_is_token_snapshot_not_raw_messages",
                "crates/mossen-cli/src/structured_io.rs",
                [
                    "fn slash_context_response",
                    '"command": "context"',
                    '"analysisDepth": "token_usage_snapshot"',
                    '"contextInputTokens"',
                    '"effectiveWindowTokens"',
                    '"autoCompactEligible"',
                    '"rawMessagesIncluded": false',
                    '"messageContentRedacted": true',
                    '"command":"/ctx"',
                ],
            ),
            source_check(
                "context_capability_manifest_available",
                "crates/mossen-agent/src/services/root/slash_command_capabilities.rs",
                [
                    '"slash.context"',
                    "ResultKind::Context",
                    "CommandStatus::Available",
                    '"ctx".to_string()',
                    '"breakdown".to_string()',
                ],
            ),
        ],
        design_note=(
            "M4.2 validates /context through the current stream-json control "
            "bridge. The checked contract is a redacted token/window snapshot, "
            "not raw message inspection or a model-generated markdown answer."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
