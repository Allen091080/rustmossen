#!/usr/bin/env python3
"""M4.5 - current Rust explicit restore-id context boundary smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M4.5",
        script_name=Path(__file__).name,
        steps=[
            Step(
                "restore_id_loads_history_without_new_session_leak",
                cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "repl::terminal_render_frontend_event_tests::oneshot_restore_id_loads_history_without_leaking_to_new_session",
                ),
                ("test result: ok.", "1 passed;"),
            ),
            Step(
                "oneshot_transcript_record_appends_turn",
                cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "repl::terminal_render_frontend_event_tests::oneshot_transcript_record_appends_turn_to_existing_history",
                ),
                ("test result: ok.", "1 passed;"),
            ),
        ],
        checks=[
            source_check(
                "restore_id_cli_flag_wired_to_repl_config",
                "crates/mossen-cli/src/main.rs",
                [
                    "restore_session_id: cli.restore_id.clone()",
                    "restore_mode: cli.should_restore()",
                ],
            ),
            source_check(
                "restore_history_enters_prompt_params",
                "crates/mossen-cli/src/repl.rs",
                [
                    "async fn load_restore_history",
                    "state.switch_session(transcript.session_id.clone())",
                    "history_messages: restore_history",
                    "SessionStartSource::Resume",
                    "async fn record_oneshot_transcript",
                ],
            ),
            source_check(
                "restore_id_flag_exists",
                "crates/mossen-cli/src/cli.rs",
                ['long = "restore-id"', "pub restore_id: Option<String>"],
            ),
        ],
        design_note=(
            "M4.5 validates the current Rust explicit restore-id boundary. "
            "It proves restore-id loads transcript messages into PromptParams "
            "and adopts the target session id, while a fresh same-cwd session "
            "starts with empty history. It also proves oneshot turns append to "
            "the transcript that future restore-id runs consume."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
