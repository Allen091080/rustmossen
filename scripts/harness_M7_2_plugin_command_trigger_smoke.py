#!/usr/bin/env python3
"""M7.2 — current Rust plugin command trigger/body smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M7.2",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="inline_plugin_command_body_is_preserved",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "plugins::plugin_loader::tests::inline_plugin_loads_and_exposes_command_body",
                ),
            ),
        ],
        checks=[
            source_check(
                "rust_command_body_is_loaded_as_prompt_content",
                "crates/mossen-utils/src/plugins/load_plugin_commands.rs",
                [
                    "let (frontmatter, body) = parse_simple_frontmatter(&content);",
                    "content_length: body.len()",
                    "content: body",
                    "user_invocable",
                    "command_type: \"prompt\".to_string()",
                ],
            ),
            source_check(
                "m72_regression_marker_in_test",
                "crates/mossen-utils/src/plugins/plugin_loader.rs",
                ["PLUGIN_M7_2_RAN", "mock_plugin_M7_1:mock_cmd_M7_1"],
            ),
        ],
        design_note=(
            "M7.2 validates command trigger readiness on the Rust command model: "
            "frontmatter is parsed, the command is user-invocable prompt source=plugin, "
            "and the markdown body marker is preserved as command content."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
