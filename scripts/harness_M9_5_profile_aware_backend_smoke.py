#!/usr/bin/env python3
"""
M9.5 — current Rust profile-to-custom-backend bridge.

The retired version of this smoke imported TS `utils/customBackend.ts` and
expected those getters to read `mossen.activeProfile` directly. The current
Rust path is different and intentional:
  - startup applies the active profile into `MOSSEN_CODE_CUSTOM_*` when those
    runtime env vars are missing;
  - explicit CLI `--model` keeps the profile model out of the custom model env;
  - provider protocol (`openai-compatible`, `openai-responses`, `anthropic`) is
    preserved in the runtime env;
  - interactive `/model <profile>` overwrites the live session env immediately.
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
            name="startup_active_profile_sets_custom_backend_env",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "active_profile_sets_custom_backend_env_for_runtime",
            ),
            timeout_secs=180,
        ),
        Step(
            name="cli_model_override_does_not_clobber_profile_backend",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "cli_model_override_keeps_profile_model_out_of_custom_model_env",
            ),
            timeout_secs=180,
        ),
        Step(
            name="startup_active_profile_preserves_provider_protocol",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "active_profile_sets_provider_protocol_for_runtime",
            ),
            timeout_secs=180,
        ),
        Step(
            name="local_model_directive_switch_updates_runtime_env",
            command=cargo_test(
                "-p",
                "mossen-commands",
                "model_directive_switches_session_profile",
            ),
            timeout_secs=180,
        ),
        Step(
            name="stream_json_model_switch_updates_runtime_env",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "slash_command_model_lists_and_switches_configured_profiles",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "startup_bridge_uses_active_profile_without_overwriting_explicit_env",
            "crates/mossen-cli/src/main.rs",
            [
                "apply_active_profile_to_custom_backend_env(cli.model.as_deref())",
                "apply_current_profile_to_custom_backend_env_if_missing(",
                "active model profile applied to custom backend environment",
            ],
        ),
        source_check(
            "interactive_profile_switch_overwrites_live_runtime_env",
            "crates/mossen-agent/src/services/config/profiles.rs",
            [
                "pub fn apply_profile_to_custom_backend_env(profile: &ListedProfile)",
                "apply_profile_to_custom_backend_env_with_mode(profile, None, true)",
                "pub fn apply_current_profile_to_custom_backend_env_if_missing(",
                "apply_profile_to_custom_backend_env_with_mode(&profile, cli_model_override, false)",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M9.5_profile_aware_backend_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M9.5 validates the current Rust bridge from configured model "
            "profiles into the custom-backend runtime environment."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
