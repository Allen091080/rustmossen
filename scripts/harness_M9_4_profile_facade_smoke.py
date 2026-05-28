#!/usr/bin/env python3
"""
M9.4 — current Rust multi-profile schema/facade contract.

The current product path is Rust `mossen-agent::services::config::profiles`,
so this harness runs focused Rust tests for:
  - reading configured profiles and active profile;
  - redacting API keys in profile inventory;
  - filtering invalid profile entries;
  - CRUD lifecycle and active-profile cascade clearing.
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
            name="profile_facade_reads_active_profile_and_redacts",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "profile_facade_reads_active_profile_and_redacts_secrets",
            ),
            timeout_secs=180,
        ),
        Step(
            name="profile_facade_filters_invalid_entries",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "profile_facade_filters_invalid_entries_and_missing_active",
            ),
            timeout_secs=180,
        ),
        Step(
            name="profile_crud_lifecycle",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "profile_crud_lifecycle_updates_active_and_preserves_raw_secret",
            ),
            timeout_secs=180,
        ),
        Step(
            name="profile_provider_protocol_variants",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "validate_profile_accepts_openai_responses_and_anthropic",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "profile_schema_supports_future_provider_protocols",
            "crates/mossen-agent/src/services/config/profiles.rs",
            [
                'OpenAiCompatible',
                'OpenAiResponses',
                'Anthropic',
                'PROFILE_PROVIDER_VALUES',
            ],
        ),
        source_check(
            "profile_inventory_desensitizes_api_keys",
            "crates/mossen-agent/src/services/config/profiles.rs",
            [
                "pub fn mask_api_key",
                "pub fn desensitize_profile",
                "pub fn desensitize_profiles",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M9.4_profile_facade_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M9.4 validates the current Rust profile facade instead of retired "
            "TS service/config imports."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
