#!/usr/bin/env python3
"""M7.4 — current Rust bad-plugin isolation smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M7.4",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="bad_inline_plugin_isolated_from_good_plugin",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::plugin_loader::tests::bad_inline_plugin_isolated_from_good_plugin",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_inline_loader_records_bad_plugin_errors",
                "crates/mossen-utils/src/plugins/plugin_loader.rs",
                [
                    "PluginError::GenericError",
                    "source: format!(\"inline[{}]\", index)",
                    "error: format!(\"Failed to load plugin: {}\", e)",
                    "plugins.push(plugin);",
                ],
            ),
            source_check(
                "m74_regression_keeps_good_command_available",
                "crates/mossen-utils/src/plugins/plugin_loader.rs",
                [
                    "M7_4_BAD",
                    "m74_good:m74_good_cmd",
                    "bad_inline_plugin_isolated_from_good_plugin",
                ],
            ),
        ],
        design_note=(
            "M7.4 validates failure isolation on the current Rust loader: a corrupt "
            "inline plugin becomes a visible inline[N] error, while the good plugin "
            "still loads and contributes commands."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
