#!/usr/bin/env python3
"""Current Rust MCP truncation fail-safe smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="MCP-truncation",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="tiny_content_skips_counter",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "mcp_validation::tests::tiny_mcp_content_does_not_call_token_counter",
                ),
            ),
            Step(
                name="large_content_broken_counter_falls_back",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "mcp_validation::tests::large_mcp_content_with_broken_counter_falls_back_to_size_estimate",
                ),
            ),
            Step(
                name="truncate_if_needed_adds_guidance",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "mcp_validation::tests::truncate_if_needed_appends_truncation_guidance",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_mcp_truncation_fails_closed_after_counter_error",
                "crates/mossen-utils/src/mcp_validation.rs",
                [
                    "MCP_TOKEN_COUNT_THRESHOLD_FACTOR",
                    "Err(e) => {",
                    "tracing::error!(\"Token counting failed: {}\", e);",
                    "size_estimate > max_tokens",
                    "large_mcp_content_with_broken_counter_falls_back_to_size_estimate",
                ],
            )
        ],
        design_note=(
            "Validates the Rust MCP truncation fail-safe: small content returns "
            "without token counting; large content with a failing counter falls "
            "back to size estimate and still truncates instead of returning false."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
