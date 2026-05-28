#!/usr/bin/env python3
"""M16.1 - hook lifecycle integration smoke.

This guards the specific failure mode where hook modules exist but the main
runtime never calls them. The checks stay focused on current Rust entrypoints
and write normal harness artifacts for the capability matrix.
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
            "post_sampling_hook_in_dialogue",
            cargo_test(
                "-p",
                "mossen-agent",
                "dialogue_executes_settings_post_sampling_hooks",
            ),
        ),
        Step(
            "pre_post_compact_hooks",
            cargo_test(
                "-p",
                "mossen-agent",
                "compact_conversation_executes_pre_and_post_compact_hooks",
            ),
        ),
        Step(
            "session_start_hooks",
            cargo_test(
                "-p",
                "mossen-cli",
                "session_hooks::tests",
            ),
        ),
        Step(
            "startup_hook_context_reaches_first_prompt",
            cargo_test(
                "-p",
                "mossen-tui",
                "startup_hook_context_drains_into_first_prompt",
            ),
        ),
        Step(
            "task_created_completed_hooks",
            [
                "cargo",
                "test",
                "-q",
                "-p",
                "mossen-tools",
                "task_",
                "--",
                "--nocapture",
                "--test-threads=1",
            ],
        ),
    ]
    checks = [
        source_check(
            "dialogue_calls_post_sampling_hooks",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "execute_post_sampling_hooks(",
                "post_sampling_manager.fire_post_inference_watchers",
                "dialogue_executes_settings_post_sampling_hooks",
            ],
        ),
        source_check(
            "compact_calls_pre_and_post_hooks",
            "crates/mossen-agent/src/services/compact/compact.rs",
            [
                "execute_pre_compact_hooks",
                "execute_post_compact_hooks",
                "compact_conversation_executes_pre_and_post_compact_hooks",
            ],
        ),
        source_check(
            "repl_wires_session_start_hook_context",
            "crates/mossen-cli/src/repl.rs",
            [
                "run_session_start_hooks",
                "with_startup_hook_messages",
                "gather_memory_text_with_hooks",
            ],
        ),
        source_check(
            "task_tools_call_lifecycle_hooks",
            "crates/mossen-tools/src/task_create.rs",
            [
                "crate::task_hooks::task_created",
                "task_create_executes_task_created_hook",
            ],
        ),
        source_check(
            "task_completion_hooks_are_blocking_capable",
            "crates/mossen-tools/src/task_update.rs",
            [
                "crate::task_hooks::task_completed",
                "task_update_blocking_hook_prevents_completion",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M16.1",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M16.1 validates that settings/plugin lifecycle hooks are called by "
            "dialogue, compact, session startup, TUI startup context, and task "
            "created/completed paths instead of merely existing as modules."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
