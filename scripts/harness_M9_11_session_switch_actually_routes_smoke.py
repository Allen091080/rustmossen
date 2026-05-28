#!/usr/bin/env python3
"""
M9.11 — `/model <profile>` must update the live request route.

The retired smoke simulated the old TS React command and AppState chain. The
current product path is Rust:
  - local `/model <profile>` is `mossen-commands::switch_model`;
  - stream-json `/model <profile>` is `mossen-cli::structured_io`;
  - both must set the session active profile, apply the profile to live
    `MOSSEN_CODE_CUSTOM_*` env, and update the main-loop model override.
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
            name="local_model_switch_updates_runtime_route",
            command=cargo_test(
                "-p",
                "mossen-commands",
                "model_directive_switches_session_profile",
            ),
            timeout_secs=180,
        ),
        Step(
            name="stream_json_model_switch_updates_main_loop_and_runtime_route",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "slash_command_model_lists_and_switches_configured_profiles",
            ),
            timeout_secs=180,
        ),
        Step(
            name="startup_active_profile_applies_to_request_env",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "active_profile_sets_custom_backend_env_for_runtime",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "stream_json_model_switch_updates_all_runtime_sources",
            "crates/mossen-cli/src/structured_io.rs",
            [
                "config_profiles::set_session_active_profile(&profile.name)",
                "config_profiles::apply_profile_to_custom_backend_env(&profile)",
                "set_main_loop_model_override(Some(profile.profile.model.clone()))",
            ],
        ),
        source_check(
            "local_model_switch_updates_live_backend_env",
            "crates/mossen-commands/src/switch_model.rs",
            [
                "config_profiles::set_session_active_profile(&profile.name)",
                "config_profiles::apply_profile_to_custom_backend_env(&profile)",
                "Switched session profile",
            ],
        ),
        source_check(
            "request_path_reads_custom_backend_model_and_base_url",
            "crates/mossen-agent/src/api_client.rs",
            [
                "custom_backend::get_custom_backend_model()",
                "custom_backend::get_custom_backend_base_url()",
                "custom_backend::get_custom_backend_auth_headers()",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M9.11_session_switch_actually_routes_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M9.11 validates that model/profile switching reaches the current "
            "Rust request route instead of only updating display state."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
