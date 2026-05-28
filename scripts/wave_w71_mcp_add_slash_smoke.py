#!/usr/bin/env python3
"""W71 — current Rust slash /mcp add and install smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W71",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="mcp_add_confirm_writes_project_config",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "bridges::tests::mcp_add_confirm_writes_project_config_and_token_is_one_shot",
                ),
            ),
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
                "slash_add_plan_engine_contract",
                "crates/mossen-agent/src/mcp/slash_add_plan.rs",
                [
                    "pub const MCP_SLASH_ADD_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;",
                    "pub fn get_mcp_slash_add_plan(",
                    "pub async fn execute_mcp_slash_add_plan(",
                    "parse_env_vars(env)",
                    "parse_headers(h)",
                    "store.remove(token)",
                ],
            ),
            source_check(
                "remote_install_plan_engine_contract",
                "crates/mossen-agent/src/mcp/remote_install_plan.rs",
                [
                    "pub const MCP_REMOTE_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;",
                    "fn to_fetchable_url(source: &str) -> String",
                    "pub async fn get_mcp_remote_install_plan(",
                    "pub async fn execute_mcp_remote_install_plan(",
                    "store.remove(token)",
                ],
            ),
            source_check(
                "slash_mcp_add_install_are_real_commands",
                "crates/mossen-commands/src/bridges.rs",
                [
                    "get_mcp_slash_add_plan(",
                    "execute_add_confirm(token, ctx).await",
                    "get_mcp_remote_install_plan(",
                    "execute_remote_confirm(token, ctx).await",
                    "FileMcpConfigWriter::new(ctx.cwd.clone())",
                    "tokio::fs::write(path, format!(\"{text}\\n\"))",
                ],
            ),
        ],
        design_note=(
            "W71 validates current Rust /mcp add and /mcp install: dry-run "
            "plan generation, one-shot confirm tokens, real project .mcp.json "
            "writes, env/header parsing, and remote JSON selection."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
