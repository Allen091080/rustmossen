#!/usr/bin/env python3
"""
M9.12 — current Rust custom-backend auth header routing by protocol.

The retired harness imported utils/customBackend.ts through Bun. The current
runtime is Rust, so this harness now proves the real Rust paths:
  - mossen-utils maps openai-compatible/openai-responses to Bearer
  - mossen-utils maps mossen-compatible/private-style traffic to x-api-key
  - explicit user auth headers keep precedence
  - the legacy non-streaming API path also uses protocol-aware headers
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
            name="custom_backend_auth_headers_protocol_contract",
            command=cargo_test("-p", "mossen-utils", "custom_backend_auth_headers"),
            timeout_secs=180,
        ),
        Step(
            name="legacy_openai_compatible_uses_bearer",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "custom_backend_non_streaming_routes_openai_compatible_uses_bearer_auth",
            ),
            timeout_secs=240,
        ),
        Step(
            name="legacy_openai_responses_uses_bearer",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "custom_backend_non_streaming_routes_openai_responses_protocol",
            ),
            timeout_secs=240,
        ),
        Step(
            name="legacy_anthropic_uses_x_api_key",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "custom_backend_non_streaming_routes_anthropic_protocol",
            ),
            timeout_secs=240,
        ),
    ]
    checks = [
        source_check(
            "legacy_api_uses_protocol_aware_custom_backend_headers",
            "crates/mossen-agent/src/api/mossen_api.rs",
            [
                "mossen_utils::custom_backend::get_custom_backend_auth_headers()",
                "custom_backend_non_streaming_routes_openai_compatible_uses_bearer_auth",
            ],
        ),
        source_check(
            "custom_backend_header_builder_preserves_user_headers",
            "crates/mossen-utils/src/custom_backend.rs",
            [
                "CustomBackendProtocol::OpenaiCompatible | CustomBackendProtocol::OpenaiResponses",
                "headers.contains_key(\"Authorization\")",
                "custom_backend_auth_headers_preserve_explicit_user_auth_header",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M9.12_auth_header_per_protocol_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M9.12 validates protocol-specific auth headers on the current Rust "
            "custom-backend path, including the legacy non-streaming fallback."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
