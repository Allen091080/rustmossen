#!/usr/bin/env python3
"""W66 — current Rust MCP template render-time i18n smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W66",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="mcp_template_inventory_current_set",
                command=cargo_test(
                    "-p",
                    "mossen-agent",
                    "mcp::builtin_templates::tests::builtin_template_inventory_contains_current_rendered_set",
                ),
            ),
            Step(
                name="mcp_template_render_time_localization",
                command=cargo_test(
                    "-p",
                    "mossen-agent",
                    "mcp::builtin_templates::tests::builtin_template_localization_is_render_time_overlay",
                ),
            ),
            Step(
                name="slash_mcp_templates_lists_localized_inventory",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "bridges::tests::mcp_templates_lists_current_rust_builtin_inventory",
                ),
            ),
        ],
        checks=[
            source_check(
                "canonical_english_templates_preserved",
                "crates/mossen-agent/src/mcp/builtin_templates.rs",
                [
                    'title: "Filesystem readonly"',
                    'description: "Template for a local filesystem MCP server scoped to explicit read-only roots."',
                    "pub fn get_localized_builtin_mcp_template_text",
                    "文件系统只读",
                    "SQLite 只读",
                ],
            ),
            source_check(
                "slash_templates_consumes_localized_overlay",
                "crates/mossen-commands/src/bridges.rs",
                [
                    "get_localized_builtin_mcp_template_text(",
                    "localized.title.unwrap_or(template.title)",
                    "localized.description.unwrap_or(template.description)",
                    "localized.notes.unwrap_or(template.notes)",
                ],
            ),
        ],
        design_note=(
            "W66 validates that Rust MCP template definitions remain canonical "
            "English while slash rendering overlays localized text at render time."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
