#!/usr/bin/env python3
"""W60 — current Rust bundled skills, plugin-dev pack, and MCP templates smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W60",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="mcp_builtin_template_lookup",
                command=cargo_test("-p", "mossen-mcp", "--lib", "plans::tests::template_lookup"),
            ),
            Step(
                name="mcp_template_missing_root_validation",
                command=cargo_test(
                    "-p",
                    "mossen-mcp",
                    "--lib",
                    "plans::tests::instantiate_missing_root",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_core_bundled_skills_registered",
                "crates/mossen-skills/src/bundled.rs",
                [
                    "pub fn register_mossen_core_skills()",
                    "\"init\"",
                    "\"review\"",
                    "\"security-review\"",
                    "pub fn init_bundled_skills()",
                    "register_simplify_skill();",
                    "register_loop_skill();",
                ],
            ),
            source_check(
                "rust_plugin_dev_pack_registered",
                "crates/mossen-cli/src/plugins.rs",
                [
                    "pub fn register_mossen_plugin_dev_plugin()",
                    "default_enabled: true",
                    "\"plugin-structure\"",
                    "\"skill-development\"",
                    "\"command-development\"",
                    "\"mcp-integration\"",
                    "\"agent-development\"",
                ],
            ),
            source_check(
                "rust_mcp_templates_are_read_only_inventory",
                "crates/mossen-mcp/src/plans.rs",
                [
                    "pub fn get_builtin_mcp_templates()",
                    "\"filesystem-readonly\"",
                    "\"git-readonly\"",
                    "\"local-docs\"",
                    "\"playwright-local\"",
                    "\"sqlite-readonly\"",
                    "Do not include production credential paths in templates.",
                ],
            ),
            source_check(
                "slash_mcp_templates_route_current_rust",
                "crates/mossen-commands/src/bridges.rs",
                [
                    "Built-in MCP templates (read-only inventory)",
                    "\"templates\" | \"template\"",
                    "format_templates_list()",
                ],
            ),
        ],
        design_note=(
            "W60 validates the Rust-era equivalents of the old TS preinstall smoke: "
            "bundled skills, plugin-dev skills, and MCP templates are registered as "
            "read-only inventory with bounded template instantiation."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
