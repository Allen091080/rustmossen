#!/usr/bin/env python3
"""M3.1 — current Rust MCP registration/list/status smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M3.1",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="mcp_config_loads_user_and_project_scopes",
                command=cargo_test(
                    "-p",
                    "mossen-mcp",
                    "config::tests::load_merged_configs_surfaces_user_and_project_scopes",
                ),
            ),
            Step(
                name="mcp_manager_registers_connected_tools",
                command=cargo_test(
                    "-p",
                    "mossen-mcp",
                    "server::tests::connect_all_keeps_good_server_when_another_server_fails",
                ),
            ),
            Step(
                name="slash_mcp_status_formats_runtime_inventory",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "bridges::tests::mcp_status_formats_all_connection_states_and_counts",
                ),
            ),
            Step(
                name="mcp_resource_tools_list_and_read_live_manager",
                command=cargo_test(
                    "-p",
                    "mossen-tools",
                    "mcp_list::tests::resource_tools_list_and_read_live_global_mcp_manager",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_config_loader_reads_user_and_project_mcp_json",
                "crates/mossen-mcp/src/config.rs",
                [
                    'global_config_dir.join("mcp.json")',
                    "get_project_mcp_file_path(cwd)",
                    "ConfigScope::User",
                    "ConfigScope::Local",
                    "merged.extend(scoped);",
                ],
            ),
            source_check(
                "runtime_status_uses_live_mcp_snapshot",
                "crates/mossen-commands/src/bridges.rs",
                [
                    "runtime_status::snapshot()",
                    "current_mcp_clients().await",
                    '"connected"',
                    '"failed"',
                ],
            ),
        ],
        design_note=(
            "M3.1 no longer shells into removed TS-era launchers. It validates the "
            "current Rust registration/list/status path: user and project mcp.json "
            "load into scoped configs, the manager exposes connected tools, and "
            "/mcp status formats the live runtime snapshot."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
