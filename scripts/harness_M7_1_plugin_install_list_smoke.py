#!/usr/bin/env python3
"""M7.1 — current Rust plugin install/list smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M7.1",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="inline_plugin_loads_and_exposes_command_body",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::plugin_loader::tests::inline_plugin_loads_and_exposes_command_body",
                ),
            ),
            Step(
                name="plugin_directive_routes_core_actions",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "plugin::tests::plugin_directive_routes_core_actions",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_loader_loads_inline_plugins",
                "crates/mossen-utils/src/plugins/plugin_loader.rs",
                [
                    "load_session_only_plugins(env, &inline_plugins).await",
                    "create_plugin_from_path(",
                    "plugin.source = format!(\"{}@inline\", plugin.name);",
                    "plugin.commands_path = Some(commands_dir);",
                ],
            ),
            source_check(
                "rust_commands_loader_lists_plugin_commands",
                "crates/mossen-utils/src/plugins/load_plugin_commands.rs",
                [
                    "pub async fn get_plugin_commands(",
                    "load_commands_from_directory(",
                    "format!(\"{}:{}\", plugin_name, command_base)",
                    "source: \"plugin\".to_string()",
                ],
            ),
            source_check(
                "rust_plugin_directive_uses_parser_and_operations",
                "crates/mossen-commands/src/plugin.rs",
                [
                    "let parsed = parse_plugin_args",
                    "run_settings_plugin_operation(",
                    "install_plugin_op(&plugin, InstallableScope::User, runtime).await",
                    "disable_plugin_op(&plugin, Some(InstallableScope::User), runtime).await",
                    "format_plugin_list(&runtime)",
                ],
            ),
            source_check(
                "cli_plugin_subcommand_routes_to_directive",
                "crates/mossen-cli/src/main.rs",
                [
                    "SubCmd::Plugin { action }",
                    "directive.execute(&[\"list\"], &ctx).await?",
                    "directive.execute(&[\"uninstall\", &name], &ctx).await?",
                    "directive.execute(&[\"enable\", &name], &ctx).await?",
                    "directive.execute(&[\"disable\", &name], &ctx).await?",
                ],
            ),
        ],
        design_note=(
            "M7.1 now validates the current Rust path: an inline --plugin-dir fixture "
            "is loaded by plugin_loader, appears as name@inline, discovers commands/, "
            "and is enumerated by get_plugin_commands."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
