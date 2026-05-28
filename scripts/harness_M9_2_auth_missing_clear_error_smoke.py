#!/usr/bin/env python3
"""
M9.2 - missing backend/auth configuration fails clearly in the current Rust path.

The retired smoke called Bun directly and could still drift into the placeholder
hosted URL. The current personal-edition contract is:
  - no configured backend fails before network retry;
  - the message points at custom-backend/profile configuration;
  - the output does not mention hosted login or api.mossen.invalid.
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
            name="streaming_missing_backend_fails_fast",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "call_streaming_without_backend_config_fails_fast_with_personal_hint",
            ),
            timeout_secs=180,
        ),
        Step(
            name="preflight_no_backend_uses_personal_hint",
            command=cargo_test(
                "-p",
                "mossen-utils",
                "endpoints_no_backend_points_to_personal_config_without_hosted_login",
            ),
            timeout_secs=180,
        ),
        Step(
            name="preflight_custom_backend_missing_url_uses_local_hint",
            command=cargo_test(
                "-p",
                "mossen-utils",
                "endpoints_custom_backend_missing_url_points_to_local_config",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "streaming_path_rejects_placeholder_backend_before_retry",
            "crates/mossen-agent/src/api_client.rs",
            [
                "MISSING_BACKEND_CONFIGURATION_MESSAGE",
                "is_placeholder_backend_url(&config.base_url)",
                "return Err(missing_backend_configuration_error())",
                "mossen.activeProfile",
            ],
        ),
        source_check(
            "preflight_error_mentions_personal_backend_not_hosted_login",
            "crates/mossen-utils/src/preflight_checks.rs",
            [
                "No Mossen backend is configured. For personal edition",
                "MOSSEN_CODE_CUSTOM_BASE_URL",
                "MOSSEN_CODE_CUSTOM_API_KEY",
                "MOSSEN_CODE_CUSTOM_AUTH_TOKEN",
                "endpoints_no_backend_points_to_personal_config_without_hosted_login",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M9.2_auth_missing_clear_error_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M9.2 validates fail-fast personal-backend configuration errors on "
            "the current Rust path, without Bun or placeholder hosted retries."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
