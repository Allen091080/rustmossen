#!/usr/bin/env python3
"""W61 — current Rust /mcp add-template contract smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W61",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="mcp_add_template_command_confirm_writes_config",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "bridges::tests::mcp_add_template_confirm_writes_instantiated_builtin_template",
                ),
            ),
            Step(
                name="mcp_builtin_template_instantiation",
                command=cargo_test(
                    "-p",
                    "mossen-agent",
                    "mcp::builtin_templates::tests::builtin_template_instantiation_replaces_absolute_parameters",
                ),
            ),
        ],
        checks=[
            source_check(
                "template_plan_token_and_confirm_path",
                "crates/mossen-agent/src/mcp/builtin_template_plan.rs",
                [
                    "pub const MCP_TEMPLATE_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;",
                    "store.remove(token)",
                    ".add_config(&plan.server_name, &plan.config, plan.scope)",
                    "Path::new(v).is_absolute()",
                ],
            ),
            source_check(
                "slash_mcp_add_template_uses_rust_plan",
                "crates/mossen-commands/src/bridges.rs",
                [
                    "get_mcp_template_install_plan(",
                    "execute_template_confirm(token, ctx).await",
                    "format_mcp_add_template_plan(&plan)",
                    "No files were modified. Confirming will write this MCP server through the existing addMcpConfig() path",
                ],
            ),
        ],
        design_note=(
            "W61 validates the Rust /mcp add-template path: dry-run plan, "
            "10-minute one-shot token, absolute path validation, confirm through "
            "addMcpConfig-compatible writer, and real .mcp.json write coverage."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
