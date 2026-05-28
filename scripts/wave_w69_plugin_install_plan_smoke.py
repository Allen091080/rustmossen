#!/usr/bin/env python3
"""W69 — current Rust plugin install dry-run/confirm smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W69",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="plugin_install_plan_resolves_and_confirms_once",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::plugin_install_plan::tests::plugin_install_plan_resolves_dependencies_and_confirm_is_one_shot",
                ),
            ),
            Step(
                name="plugin_install_plan_rejects_invalid_inputs",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::plugin_install_plan::tests::plugin_install_plan_rejects_missing_scope_and_blocked_dependency",
                ),
            ),
            Step(
                name="plugin_directive_install_plan_uses_cached_marketplace_and_confirm_token",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "plugin::tests::plugin_directive_install_plan_uses_cached_marketplace_and_confirm_token",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_plugin_install_plan_contract",
                "crates/mossen-utils/src/plugins/plugin_install_plan.rs",
                [
                    "pub const PLUGIN_INSTALL_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;",
                    "pub async fn get_plugin_install_plan(",
                    "pub async fn execute_plugin_install_plan(",
                    "resolve_dependency_closure(",
                    "store.remove(token)",
                    "reset_plugin_install_plan_store_for_testing()",
                ],
            ),
            source_check(
                "plugin_plan_does_not_write_during_dry_run",
                "crates/mossen-utils/src/plugins/plugin_install_plan.rs",
                [
                    "PluginInstallResolver",
                    "PluginInstaller",
                    "install_resolved_plugin(&plan).await",
                ],
            ),
            source_check(
                "rust_plugin_directive_routes_install_plan",
                "crates/mossen-commands/src/plugin.rs",
                [
                    "ParsedCommand::InstallPlan",
                    "get_plugin_install_plan(plugin.as_deref(), Some(scope), &resolver).await",
                    "execute_plugin_install_plan(&token, &installer).await",
                    "read_marketplace_from_location",
                ],
            ),
            source_check(
                "run_all_keeps_w69_registered",
                "scripts/run_all_smoke.sh",
                ["wave_w69_plugin_install_plan_smoke.py"],
            ),
        ],
        design_note=(
            "W69 now validates the Rust install-plan contract. Dry-run creates a "
            "tokenized dependency-aware plan without writes, confirm consumes the "
            "one-shot token through PluginInstaller, and invalid inputs/policy blocks "
            "are executable tests."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
