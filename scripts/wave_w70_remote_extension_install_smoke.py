#!/usr/bin/env python3
"""W70 — current Rust remote install surfaces smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W70",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="mcp_remote_install_confirm_writes_selected_server",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "bridges::tests::mcp_remote_install_confirm_writes_selected_server",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_mcp_remote_plan_contract",
                "crates/mossen-agent/src/mcp/remote_install_plan.rs",
                [
                    "pub const MCP_REMOTE_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;",
                    "pub async fn get_mcp_remote_install_plan(",
                    "pub async fn execute_mcp_remote_install_plan(",
                    "serde_json::from_value(config_val.clone())",
                    "store.remove(token)",
                    ".add_config(&plan.server_name, &plan.config, plan.scope)",
                    "https://raw.githubusercontent.com",
                ],
            ),
            source_check(
                "slash_mcp_install_routes_to_remote_plan",
                "crates/mossen-commands/src/bridges.rs",
                [
                    '"install" =>',
                    "get_mcp_remote_install_plan(",
                    "execute_mcp_remote_install_plan(token, &writer)",
                    "MCP remote install dry-run",
                    "Installed remote MCP server",
                ],
            ),
            source_check(
                "rust_plugin_github_direct_plan_surface_present",
                "crates/mossen-utils/src/plugins/plugin_install_plan.rs",
                [
                    "const GITHUB_DIRECT_MARKETPLACE: &str = \"github-direct\";",
                    "parse_github_plugin_target(",
                    "load_github_plugin_manifest(",
                    "execute_plugin_install_plan(",
                    "reset_plugin_install_plan_store_for_testing()",
                ],
            ),
            source_check(
                "run_all_keeps_w70_registered",
                "scripts/run_all_smoke.sh",
                ["wave_w70_remote_extension_install_smoke.py"],
            ),
        ],
        design_note=(
            "W70 was migrated from stale TS path checks to current Rust evidence. "
            "The executable assertion covers /mcp install dry-run/confirm writing "
            "through the remote plan; source checks keep the GitHub plugin direct "
            "surface and run_all registration visible without requiring removed TS files."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
