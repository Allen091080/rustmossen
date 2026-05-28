#!/usr/bin/env python3
"""W68 — current Rust MCP status visibility smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W68",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="slash_mcp_status_formats_connection_counts",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "bridges::tests::mcp_status_formats_all_connection_states_and_counts",
                ),
            ),
            Step(
                name="stream_json_mcp_inventory_redacted_snapshot",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "structured_io::tests::slash_command_mcp_inventory_returns_redacted_snapshot",
                ),
            ),
            Step(
                name="tui_slash_mcp_opens_detail_panel",
                command=cargo_test(
                    "-p",
                    "mossen-tui",
                    "builtin_slash_commands_open_expected_ui",
                ),
            ),
        ],
        checks=[
            source_check(
                "mcp_status_reads_runtime_snapshot",
                "crates/mossen-commands/src/bridges.rs",
                [
                    "runtime_status::snapshot()",
                    "RuntimeMcpConnectionState::NeedsAuth",
                    "format_mcp_status(&clients)",
                    "This command does not reconnect, enable, disable, authenticate, or modify MCP config.",
                ],
            ),
            source_check(
                "stream_json_mcp_status_redaction",
                "crates/mossen-cli/src/structured_io.rs",
                [
                    "mcp_runtime_status::snapshot().await",
                    '"rawConfigRedacted"',
                    '"toolSchemasRedacted"',
                    '"instructionsRedacted"',
                    '"errorDetailsRedacted"',
                    '"mutationSupported"',
                ],
            ),
            source_check(
                "tui_mcp_modal_registered",
                "crates/mossen-tui/src/app.rs",
                [
                    '"mcp" => {',
                    "self.refresh_mcp_statuses();",
                    "ActiveModal::McpServersDialog",
                ],
            ),
        ],
        design_note=(
            "W68 validates MCP visibility in Rust: /mcp status formatting, "
            "stream-json redacted runtime snapshot, and TUI /mcp detail panel routing."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
