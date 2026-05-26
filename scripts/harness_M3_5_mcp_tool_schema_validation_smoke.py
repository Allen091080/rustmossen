#!/usr/bin/env python3
"""M3.5 — current Rust MCP schema/error propagation smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M3.5",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="mcp_client_preserves_server_side_schema_error",
                command=cargo_test(
                    "-p",
                    "mossen-mcp",
                    "client::tests::client_routes_transport_responses_to_pending_requests",
                ),
            ),
            Step(
                name="mcp_tool_result_conversion_preserves_error_marker",
                command=cargo_test(
                    "-p",
                    "mossen-mcp",
                    "tools::tests::call_result_conversion_preserves_server_error_marker",
                ),
            ),
            Step(
                name="mcp_auth_bridge_fails_closed_when_runtime_flow_unavailable",
                command=cargo_test(
                    "-p",
                    "mossen-tools",
                    "mcp_auth::tests::bridge_authenticator_fails_closed_when_runtime_flow_is_unavailable",
                ),
            ),
            Step(
                name="mcp_oauth_plan_and_redaction_contract",
                command=cargo_test(
                    "-p",
                    "mossen-agent",
                    "mcp::auth::tests::",
                ),
            ),
        ],
        checks=[
            source_check(
                "server_is_error_becomes_internal_tool_error",
                "crates/mossen-mcp/src/tools.rs",
                [
                    "is_error: result.is_error.unwrap_or(false)",
                    "MISSING_REQUIRED_text_M3_5",
                    "call_result_conversion_preserves_server_error_marker",
                ],
            ),
            source_check(
                "dialogue_returns_mcp_error_to_model_not_empty_success",
                "crates/mossen-agent/src/dialogue.rs",
                [
                    "is_error: result.is_error",
                    "Error: MCP call to",
                    "is_error: true",
                ],
            ),
        ],
        design_note=(
            "M3.5 validates schema-error behavior at the MCP boundary: missing "
            "required arguments remain a server-side tools/call error with the "
            "MISSING_REQUIRED_text_M3_5 marker and are converted into an internal "
            "tool result with is_error=true instead of a silent success."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
