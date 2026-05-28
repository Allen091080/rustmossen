#!/usr/bin/env python3
"""M3.4 — current Rust MCP scope visibility and failed-server isolation smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M3.4",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="mcp_scope_loader_surfaces_user_project_and_override",
                command=cargo_test(
                    "-p",
                    "mossen-mcp",
                    "config::tests::load_merged_configs_surfaces_user_and_project_scopes",
                ),
            ),
            Step(
                name="bad_server_failure_does_not_drop_good_server",
                command=cargo_test(
                    "-p",
                    "mossen-mcp",
                    "server::tests::connect_all_keeps_good_server_when_another_server_fails",
                ),
            ),
        ],
        checks=[
            source_check(
                "config_scope_merge_order_is_user_then_project",
                "crates/mossen-mcp/src/config.rs",
                [
                    "按优先级从低到高合并：enterprise < user < project < local < dynamic",
                    'global_config_dir.join("mcp.json")',
                    "let project_config_path = get_project_mcp_file_path(cwd);",
                    "ConfigScope::User",
                    "ConfigScope::Local",
                ],
            ),
            source_check(
                "connect_all_isolates_per_server_failures",
                "crates/mossen-mcp/src/server.rs",
                [
                    "pub async fn connect_all(&self)",
                    "let _ = self.connect_server(&name).await;",
                    "McpServerConnection::Failed(FailedServer",
                    "self.clients.insert(name.to_string(), Arc::new(client));",
                ],
            ),
        ],
        design_note=(
            "M3.4 validates the Rust scope and isolation contract directly: user "
            "and project MCP configs are both visible, project overrides user for "
            "duplicate names, and a failed stdio spawn is recorded as Failed while "
            "the good server remains connected and callable."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
