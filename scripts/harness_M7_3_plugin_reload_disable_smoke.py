#!/usr/bin/env python3
"""M7.3 — current Rust plugin reload/disable smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M7.3",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="inline_plugin_reload_and_disable_updates_commands",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::plugin_loader::tests::inline_plugin_reload_and_disable_updates_commands",
                ),
            ),
        ],
        checks=[
            source_check(
                "command_cache_can_be_cleared_for_reload",
                "crates/mossen-utils/src/plugins/load_plugin_commands.rs",
                [
                    "static PLUGIN_COMMAND_CACHE",
                    "pub fn clear_plugin_command_cache()",
                    "*PLUGIN_COMMAND_CACHE.lock().unwrap() = None;",
                ],
            ),
            source_check(
                "m73_regression_covers_reload_and_disable",
                "crates/mossen-utils/src/plugins/plugin_loader.rs",
                [
                    "cmd_v1_M7_3",
                    "cmd_v2_M7_3",
                    "env.set_inline_plugins(Vec::new());",
                    "disabled.enabled.is_empty()",
                ],
            ),
        ],
        design_note=(
            "M7.3 validates Rust plugin reload behavior: command cache clearing makes "
            "a newly added command visible, and removing inline plugin dirs removes the "
            "plugin plus all of its commands."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
