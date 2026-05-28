#!/usr/bin/env python3
"""
M9.3 - current Rust model override smoke.

The retired smoke launched an old wrapper against a real external model and
then scraped session logs. The current contract is covered by focused Rust
gates:
  - CLI --model is written into startup state;
  - active profile application does not overwrite a CLI model override through
    MOSSEN_CODE_CUSTOM_MODEL;
  - /model in stream-json updates the main-loop override and can reset it.
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
            name="cli_model_override_sets_startup_state",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "cli_model_override_is_applied_to_startup_state",
            ),
            timeout_secs=180,
        ),
        Step(
            name="cli_model_override_keeps_profile_model_out_of_custom_env",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "cli_model_override_keeps_profile_model_out_of_custom_model_env",
            ),
            timeout_secs=180,
        ),
        Step(
            name="slash_model_override_updates_main_loop",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "slash_command_model_reports_and_updates_override",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "cli_model_override_is_applied_before_repl_or_oneshot",
            "crates/mossen-cli/src/main.rs",
            [
                "apply_active_profile_to_custom_backend_env(cli.model.as_deref())",
                "s.model_override = Some(model.clone())",
                "model_override: cli.model.clone()",
                "cli_model_override_is_applied_to_startup_state",
            ],
        ),
        source_check(
            "profile_bridge_respects_cli_model_override",
            "crates/mossen-agent/src/services/config/profiles.rs",
            [
                "cli_model_override: Option<&str>",
                "if cli_model_override",
                "\"MOSSEN_CODE_CUSTOM_MODEL\"",
                "&profile.profile.model",
            ],
        ),
        source_check(
            "stream_json_model_command_updates_override",
            "crates/mossen-cli/src/structured_io.rs",
            [
                "set_main_loop_model_override(Some(requested_model))",
                "set_main_loop_model_override(Some(profile.profile.model.clone()))",
                "set_main_loop_model_override(None)",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M9.3_model_override_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M9.3 validates model override plumbing on the current Rust CLI and "
            "stream-json paths, without retired wrappers or external LLM calls."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
