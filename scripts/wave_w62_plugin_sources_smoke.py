#!/usr/bin/env python3
"""W62 — current Rust plugin sources visibility smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W62",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="describe_plugin_sources_merges_sources",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::source_status::tests::describe_plugin_sources_merges_declared_known_and_official_status",
                ),
            ),
            Step(
                name="plugin_parse_args_sources_paths",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "plugin_parse_args::tests::test_prune_sources_paths_and_plan_commands",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_source_status_contract",
                "crates/mossen-utils/src/plugins/source_status.rs",
                [
                    "pub async fn describe_plugin_sources(",
                    "DeclaredMarketplaceInfo",
                    "KnownMarketplaceInfo",
                    "OFFICIAL_MARKETPLACE_NAME",
                    "/plugin marketplace list",
                    "/plugin status",
                ],
            ),
            source_check(
                "marketplace_source_display_helper_reused",
                "crates/mossen-utils/src/plugins/marketplace_helpers.rs",
                ["pub fn get_marketplace_source_display(", "MarketplaceSource::GitHub"],
            ),
            source_check(
                "rust_plugin_parse_args_routes_sources",
                "crates/mossen-commands/src/plugin_parse_args.rs",
                ["Status", "Sources", "Paths", "\"sources\" | \"source\"", "\"paths\" | \"path\""],
            ),
            source_check(
                "run_all_keeps_w62_registered",
                "scripts/run_all_smoke.sh",
                ["wave_w62_plugin_sources_smoke.py"],
            ),
        ],
        design_note=(
            "W62 now validates the Rust read-only source-status surface: declared, "
            "known, seed, cache, and official marketplace data are merged without "
            "mutating plugin state, and /plugin sources/paths parsing is covered."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
