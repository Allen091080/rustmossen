#!/usr/bin/env python3
"""M3.2 — current Rust MCP tool-call routing smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M3.2",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="mcp_client_calls_tool_and_preserves_payload",
                command=cargo_test(
                    "-p",
                    "mossen-mcp",
                    "client::tests::client_routes_transport_responses_to_pending_requests",
                ),
            ),
            Step(
                name="mcp_manager_routes_live_stdio_tool_call",
                command=cargo_test(
                    "-p",
                    "mossen-mcp",
                    "server::tests::connect_all_keeps_good_server_when_another_server_fails",
                ),
            ),
        ],
        checks=[
            source_check(
                "dialogue_routes_model_visible_mcp_tool_to_global_manager",
                "crates/mossen-agent/src/dialogue.rs",
                [
                    "async fn execute_mcp_tool(",
                    "mossen_mcp::tools::parse_mcp_tool_name(qualified_name)",
                    "mossen_mcp::server::global_manager()",
                    "get_client_by_normalized_name(&server)",
                    "execute_mcp_tool_call(&client, &original_tool_name, arguments).await",
                ],
            ),
            source_check(
                "mcp_tool_call_executes_client_call_tool",
                "crates/mossen-mcp/src/tools.rs",
                [
                    "pub async fn execute_mcp_tool_call(",
                    "client.call_tool(tool_name, arguments).await?",
                    "is_error: result.is_error.unwrap_or(false)",
                ],
            ),
        ],
        design_note=(
            "M3.2 validates the current Rust MCP tool-call path without relying on "
            "LLM behavior: model-visible mcp__server__tool names resolve to the "
            "global manager, route to the original MCP tool name, and preserve the "
            "mock server payload returned by tools/call."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
