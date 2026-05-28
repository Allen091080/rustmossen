#!/usr/bin/env python3
"""
M9.13 - provider mock protocol matrix for V1.1 External User Ready.

This harness makes the three supported provider contracts explicit:
  - openai-compatible Chat Completions
  - openai-responses Responses API
  - anthropic Messages API

The underlying tests use local mock HTTP servers, not real external providers.
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
            name="profile_schema_accepts_all_public_provider_protocols",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "validate_profile_accepts_openai_responses_and_anthropic",
            ),
            timeout_secs=180,
        ),
        Step(
            name="auth_headers_route_by_provider_protocol",
            command=cargo_test("-p", "mossen-utils", "custom_backend_auth_headers"),
            timeout_secs=180,
        ),
        Step(
            name="non_streaming_openai_compatible_mock",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "custom_backend_non_streaming_routes_openai_compatible_uses_bearer_auth",
            ),
            timeout_secs=240,
        ),
        Step(
            name="non_streaming_openai_responses_mock",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "custom_backend_non_streaming_routes_openai_responses_protocol",
            ),
            timeout_secs=240,
        ),
        Step(
            name="non_streaming_anthropic_mock",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "custom_backend_non_streaming_routes_anthropic_protocol",
            ),
            timeout_secs=240,
        ),
        Step(
            name="streaming_openai_compatible_tool_loop_mock",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "harness_executes_glob_and_continues_after_openai_compatible_tool_result",
            ),
            timeout_secs=240,
        ),
        Step(
            name="streaming_openai_responses_endpoint_mock",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "harness_routes_openai_responses_protocol_to_responses_endpoint",
            ),
            timeout_secs=240,
        ),
        Step(
            name="streaming_openai_responses_tool_loop_mock",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "harness_executes_tool_loop_through_openai_responses_protocol",
            ),
            timeout_secs=240,
        ),
        Step(
            name="streaming_anthropic_endpoint_mock",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "harness_routes_anthropic_protocol_to_messages_endpoint",
            ),
            timeout_secs=240,
        ),
        Step(
            name="streaming_anthropic_tool_loop_mock",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "harness_executes_tool_loop_through_anthropic_protocol",
            ),
            timeout_secs=240,
        ),
        Step(
            name="startup_profile_bridge_preserves_provider_protocol",
            command=cargo_test(
                "-p",
                "mossen-cli",
                "active_profile_sets_provider_protocol_for_runtime",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "provider_schema_lists_public_protocols",
            "crates/mossen-agent/src/services/config/profiles.rs",
            [
                'OpenAiCompatible => "openai-compatible"',
                'OpenAiResponses => "openai-responses"',
                'Anthropic => "anthropic"',
                "validate_profile_accepts_openai_responses_and_anthropic",
            ],
        ),
        source_check(
            "streaming_dialogue_has_local_mock_protocol_matrix",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "harness_executes_glob_and_continues_after_openai_compatible_tool_result",
                "harness_routes_openai_responses_protocol_to_responses_endpoint",
                "harness_executes_tool_loop_through_openai_responses_protocol",
                "harness_routes_anthropic_protocol_to_messages_endpoint",
                "harness_executes_tool_loop_through_anthropic_protocol",
            ],
        ),
        source_check(
            "legacy_api_has_local_mock_protocol_matrix",
            "crates/mossen-agent/src/api/mossen_api.rs",
            [
                "custom_backend_non_streaming_routes_openai_compatible_uses_bearer_auth",
                "custom_backend_non_streaming_routes_openai_responses_protocol",
                "custom_backend_non_streaming_routes_anthropic_protocol",
            ],
        ),
        source_check(
            "full_tests_workflow_runs_provider_mock_matrix",
            ".github/workflows/tests.yml",
            ["python3 scripts/harness_M9_13_provider_mock_protocol_matrix.py"],
        ),
    ]
    return run_context_harness(
        test_id="M9.13_provider_mock_protocol_matrix",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "V1.1 provider gate: local mock coverage for openai-compatible, "
            "openai-responses, and anthropic protocols across schema, auth, "
            "legacy API routing, streaming endpoints, tool-loop continuation, "
            "and startup profile env bridging."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
