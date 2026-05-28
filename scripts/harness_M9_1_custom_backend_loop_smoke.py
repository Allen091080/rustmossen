#!/usr/bin/env python3
"""
M9.1 - current Rust custom-backend agent loop smoke.

The retired harness launched an old wrapper with a real DashScope key. That was
not a reliable production gate: it depended on an external model, an obsolete
entrypoint, and a leaked-looking fixture credential. The current smoke validates
the live Rust chain with local mock servers:
  - custom backend routing selects OpenAI-compatible streaming;
  - the dialogue loop executes a Glob tool call;
  - the tool_result is sent back to the model;
  - the model continues to a final answer after the tool result.
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
            name="openai_compatible_agent_loop_executes_tool_and_continues",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "harness_executes_glob_and_continues_after_openai_compatible_tool_result",
            ),
            timeout_secs=240,
        ),
        Step(
            name="openai_compatible_custom_backend_uses_protocol_auth",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "custom_backend_non_streaming_routes_openai_compatible_uses_bearer_auth",
            ),
            timeout_secs=240,
        ),
    ]
    checks = [
        source_check(
            "dialogue_harness_replays_openai_tool_result",
            "crates/mossen-agent/src/dialogue.rs",
            [
                "harness_executes_glob_and_continues_after_openai_compatible_tool_result",
                "EnvRestore::set_custom_backend(&base_url)",
                "ToolUseSummary",
                "second request should include OpenAI tool result message",
                "harness completed after glob",
            ],
        ),
        source_check(
            "streaming_path_routes_custom_backend_to_openai_compat",
            "crates/mossen-agent/src/api_client.rs",
            [
                "custom backend routing selected",
                "call_streaming_openai_compat(params, cancel).await",
                "custom_backend::get_custom_backend_model().unwrap_or_else(|| params.model.clone())",
                "custom_backend::get_custom_backend_auth_headers()",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M9.1_custom_backend_loop_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M9.1 validates the current Rust custom-backend tool loop with "
            "mock servers, avoiding retired wrappers and real external LLM calls."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
