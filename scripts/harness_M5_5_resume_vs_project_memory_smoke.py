#!/usr/bin/env python3
"""M5.5 — resume transcript history and project memory remain separate."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M5.5",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="restore_history_and_project_memory_stay_separate",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "repl::terminal_render_frontend_event_tests::restore_history_and_project_memory_stay_separate",
                ),
            )
        ],
        checks=[
            source_check(
                "oneshot_params_load_history_and_system_memory_separately",
                "crates/mossen-cli/src/repl.rs",
                [
                    "let restore_history = load_restore_history",
                    "gather_memory_text_with_hooks(&cwd_path, hook_context.as_deref())",
                    "history_messages: restore_history",
                    "system_prompt: system_prompt_blocks",
                ],
            )
        ],
        design_note=(
            "M5.5 validates that explicit resume history goes into history_messages "
            "while project MOSSEN.md goes into the system prompt; fresh sessions "
            "load project memory but do not inherit the resumed conversation."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
