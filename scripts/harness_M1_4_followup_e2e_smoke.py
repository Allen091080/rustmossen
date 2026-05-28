#!/usr/bin/env python3
"""
M1.4 - current Rust restore/follow-up context smoke.

The old script relied on two real LLM invocations and `--continue`. The current
gate validates the deterministic restore pipeline: explicit restore-id history
is loaded into oneshot prompt params, fresh sessions do not inherit it, and
conversation history stays separate from project memory.
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
            name="restore_id_loads_history_without_leaking_to_new_session",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "oneshot_restore_id_loads_history_without_leaking_to_new_session",
            ),
            timeout_secs=180,
        ),
        Step(
            name="restore_history_and_project_memory_are_separate",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "restore_history_and_project_memory_stay_separate",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "oneshot_restore_history_is_loaded_before_prompt_params",
            "crates/mossen-cli/src/repl.rs",
            [
                "load_restore_history(config, &cwd, &model, &state).await?",
                "history_messages: restore_history",
                "oneshot_restore_id_loads_history_without_leaking_to_new_session",
            ],
        ),
        source_check(
            "fresh_session_does_not_implicitly_resume",
            "crates/mossen-cli/src/repl.rs",
            [
                "if !config.restore_mode && config.restore_session_id.is_none()",
                "new sessions must not adopt explicit restore-id history",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M1.4_followup_restore_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M1.4 validates current Rust restore/follow-up context handling "
            "without real LLM calls."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
