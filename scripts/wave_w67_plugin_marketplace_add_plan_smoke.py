#!/usr/bin/env python3
"""W67 — current Rust plugin marketplace add dry-run/confirm smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W67",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="marketplace_add_plan_confirms_once",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::marketplace_add_plan::tests::marketplace_add_plan_confirms_once_and_clears_caches",
                ),
            ),
            Step(
                name="marketplace_add_plan_rejects_invalid_sources",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::marketplace_add_plan::tests::marketplace_add_plan_rejects_missing_and_invalid_sources",
                ),
            ),
            Step(
                name="plugin_parse_args_marketplace_add_plan",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "plugin_parse_args::tests::test_prune_sources_paths_and_plan_commands",
                ),
            ),
            Step(
                name="plugin_directive_marketplace_add_plan_writes_settings",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "plugin::tests::plugin_directive_marketplace_add_plan_writes_settings",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_marketplace_add_plan_contract",
                "crates/mossen-utils/src/plugins/marketplace_add_plan.rs",
                [
                    "pub const PLUGIN_MARKETPLACE_ADD_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;",
                    "pub async fn get_plugin_marketplace_add_plan(",
                    "pub async fn execute_plugin_marketplace_add_plan(",
                    "store.remove(token)",
                    "save_marketplace_to_settings",
                    "clear_all_caches",
                    "reset_plugin_marketplace_add_plan_store_for_testing()",
                ],
            ),
            source_check(
                "rust_marketplace_input_parser_present",
                "crates/mossen-utils/src/plugins/parse_marketplace_input.rs",
                [
                    "pub async fn parse_marketplace_input(",
                    "MarketplaceSource::GitHub",
                    "MarketplaceSource::Git",
                    "MarketplaceSource::Directory",
                    "MarketplaceSource::File",
                ],
            ),
            source_check(
                "rust_plugin_parse_args_routes_marketplace_add_plan",
                "crates/mossen-commands/src/plugin_parse_args.rs",
                [
                    "MarketplaceAddPlan",
                    "\"marketplace\" | \"market\"",
                    "Some(\"add\")",
                    "\"--dry-run\"",
                    "\"--confirm\"",
                ],
            ),
            source_check(
                "rust_plugin_directive_routes_marketplace_add_plan",
                "crates/mossen-commands/src/plugin.rs",
                [
                    "ParsedCommand::MarketplaceAddPlan",
                    "run_marketplace_add_plan(target, confirm_token, &runtime).await",
                    "execute_plugin_marketplace_add_plan(",
                    "save_marketplace_to_user_settings",
                ],
            ),
            source_check(
                "run_all_keeps_w67_registered",
                "scripts/run_all_smoke.sh",
                ["wave_w67_plugin_marketplace_add_plan_smoke.py"],
            ),
        ],
        design_note=(
            "W67 now validates the Rust marketplace-add plan: dry-run parses a "
            "marketplace source into a tokenized plan, confirm writes via the injected "
            "settings callback, clears caches, and consumes the token exactly once."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
