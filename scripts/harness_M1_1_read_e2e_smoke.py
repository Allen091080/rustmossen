#!/usr/bin/env python3
"""
M1.1 - current Rust Read tool smoke.

The retired version depended on a real model deciding to call Read through an
old launcher. This gate now validates the deterministic Rust pieces that matter:
the Read tool resolves paths from ToolUseContext and the dialogue loop can carry
tool output back through a provider-compatible turn.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    steps = [
        Step(
            name="read_tool_reads_relative_path_from_context",
            command=cargo_test(
                "-p",
                "mossen-tools",
                "read_relative_path_resolves_against_tool_context_cwd",
            ),
            timeout_secs=180,
        ),
        Step(
            name="dialogue_tool_result_reaches_model",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "harness_executes_glob_and_continues_after_openai_compatible_tool_result",
            ),
            timeout_secs=240,
        ),
    ]
    checks = [
        source_check(
            "read_tool_registered_as_read_and_uses_context_cwd",
            "crates/mossen-tools/src/file_read.rs",
            [
                "fn name(&self) -> &str",
                "\"Read\"",
                "fn resolve_tool_path(path: &str, cwd: &str) -> String",
                "PathBuf::from(cwd).join(path)",
                "read_relative_path_resolves_against_tool_context_cwd",
            ],
        ),
        source_check(
            "dialogue_executes_registered_tool_and_emits_summary",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "tool_registry",
                ".execute_with_cancel(tool_name, input, &tool_use_context, cancel)",
                "SdkMessage::ToolUseSummary",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M1.1_read_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M1.1 validates current Rust Read tool execution and tool-result "
            "continuation without relying on a real LLM or retired launcher."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
