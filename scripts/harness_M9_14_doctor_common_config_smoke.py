#!/usr/bin/env python3
"""M9.14 - /doctor common model configuration diagnostics smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    steps = [
        Step(
            name="doctor_common_config_diagnostics",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "slash_command_doctor",
            ),
            timeout_secs=240,
        ),
    ]
    checks = [
        source_check(
            "doctor_snapshot_has_common_config_issue_codes",
            "crates/mossen-agent/src/services/config/doctor.rs",
            [
                "profiles_not_object",
                "no_valid_settings_profiles",
                "some_settings_profiles_invalid",
                "active_profile_not_found",
                "no_model_profile",
                "custom_backend_env_incomplete",
                "base_urls_redacted: true",
                "api_keys_redacted: true",
            ],
        ),
        source_check(
            "structured_doctor_tests_cover_common_model_config_failures",
            "crates/mossen-cli/src/structured_io.rs",
            [
                "slash_command_doctor_guides_when_model_profile_is_missing",
                "slash_command_doctor_warns_when_active_profile_is_missing",
                "slash_command_doctor_reports_invalid_model_profiles",
                "slash_command_doctor_reports_partial_custom_backend_env",
                "slash_command_doctor_redacts_configured_model_profile_secrets",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M9.14_doctor_common_config_diagnostics",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M9.14 validates that /doctor identifies common model configuration "
            "failures for external users and keeps raw API keys/base URLs redacted."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
