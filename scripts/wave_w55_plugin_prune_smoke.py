#!/usr/bin/env python3
"""W55 — current Rust plugin prune smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W55",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="plugin_prune_plan_marks_and_deletes_orphans",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::cache_utils::tests::plugin_prune_plan_marks_unmarked_deletes_expired_and_is_one_shot",
                ),
            ),
            Step(
                name="plugin_prune_plan_refuses_zip_cache_mode",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::cache_utils::tests::plugin_prune_plan_refuses_zip_cache_mode",
                ),
            ),
            Step(
                name="plugin_parse_args_prune_and_plan_commands",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "plugin_parse_args::tests::test_prune_sources_paths_and_plan_commands",
                ),
            ),
            Step(
                name="plugin_directive_surfaces_prune_status_sources_paths",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "plugin::tests::plugin_directive_surfaces_status_sources_paths_and_prune",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_prune_plan_contract",
                "crates/mossen-utils/src/plugins/cache_utils.rs",
                [
                    "pub const PRUNE_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;",
                    "pub async fn get_plugin_prune_plan(",
                    "pub async fn execute_plugin_prune_plan(",
                    "store.remove(token)",
                    "CLEANUP_AGE_MS",
                    "reset_prune_plan_store_for_testing()",
                ],
            ),
            source_check(
                "rust_plugin_parse_args_routes_prune",
                "crates/mossen-commands/src/plugin_parse_args.rs",
                [
                    "Prune { confirm_token: Option<String> }",
                    "\"prune\" =>",
                    "parts.get(idx + 1)",
                ],
            ),
            source_check(
                "rust_plugin_directive_routes_prune",
                "crates/mossen-commands/src/plugin.rs",
                [
                    "ParsedCommand::Prune { confirm_token } => run_prune(confirm_token, &runtime).await",
                    "get_plugin_prune_plan(&runtime.cache_dir(), &installed_paths, false).await",
                    "execute_plugin_prune_plan(&token, &installed_paths).await",
                ],
            ),
            source_check(
                "run_all_keeps_w55_registered",
                "scripts/run_all_smoke.sh",
                ["wave_w55_plugin_prune_smoke.py"],
            ),
        ],
        design_note=(
            "W55 now validates the Rust prune-plan path: dry-run classifies expired, "
            "unmarked, fresh, and installed-skipped cache versions; confirm consumes "
            "a one-shot token, marks unmarked versions, deletes expired orphans, and "
            "refuses zip-cache mode."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
