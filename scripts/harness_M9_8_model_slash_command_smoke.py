#!/usr/bin/env python3
"""
M9.8 — current Rust /model and /profile slash-command profile contract.

This used to import the retired TS/Bun command implementation directly. The
current product path is Rust stream-json slash-command handling, so this harness
now runs the focused Rust gates that exercise the real control-request router:
  - /model status, raw model override, and reset
  - /model list with configured profiles, profile switch, env bridge, redaction
  - /profile list/use with session-only mutation and redacted inventory
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
            name="model_status_raw_override_and_reset",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "slash_command_model_reports_and_updates_override",
            ),
            timeout_secs=180,
        ),
        Step(
            name="model_profile_list_switch_and_redaction",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "slash_command_model_lists_and_switches_configured_profiles",
            ),
            timeout_secs=180,
        ),
        Step(
            name="profile_list_use_session_only_and_redaction",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "slash_command_profile_lists_and_switches_session_profile",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "model_switch_updates_session_profile_runtime_and_main_loop",
            "crates/mossen-cli/src/structured_io.rs",
            [
                "config_profiles::set_session_active_profile(&profile.name)",
                "config_profiles::apply_profile_to_custom_backend_env(&profile)",
                "set_main_loop_model_override(Some(profile.profile.model.clone()))",
                '"apiKeyRedacted": true',
                '"baseUrlRedacted": true',
            ],
        ),
        source_check(
            "profile_switch_is_session_only",
            "crates/mossen-agent/src/services/config/profiles.rs",
            [
                "pub fn set_session_active_profile(",
                "set_active_profile(name, ConfigOverrideScope::Override)",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M9.8_model_slash_command_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M9.8 validates the current Rust stream-json slash-command path. "
            "It intentionally no longer imports retired TS/Bun command files."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
