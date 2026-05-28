#!/usr/bin/env python3
"""W79 - auto-compact query replay smoke.

Guards the long-session context path: when auto-compact triggers, the next
model request must use the compacted message list, not just emit a UI event.
"""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
CONTEXT = ROOT / "crates/mossen-agent/src/context/mod.rs"
DIALOGUE = ROOT / "crates/mossen-agent/src/dialogue.rs"
SERVICE_AUTO = ROOT / "crates/mossen-agent/src/services/compact/auto_compact.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def main() -> int:
    failures: list[str] = []
    context = CONTEXT.read_text()
    dialogue = DIALOGUE.read_text()
    service_auto = SERVICE_AUTO.read_text()
    run_all = RUN_ALL.read_text()

    for snippet in [
        "messages: Vec<Message>",
        "let compacted_messages = result.new_messages;",
        "t.last_compact_token_count = Some(after_tokens);",
        "t.last_compact_time = Some(chrono::Utc::now());",
        "auto_compact_returns_compacted_messages_and_updates_tracking",
    ]:
        if snippet not in context:
            fail(failures, f"context auto-compact missing snippet: {snippet}")

    for snippet in [
        "let mut messages_for_query = match compact_result",
        "messages: compacted_messages",
        "state.messages = compacted_messages.clone();",
        "AutoCompactResult::Failed { error }",
        "yield_missing_tool_result_blocks(&messages_for_query)",
    ]:
        if snippet not in dialogue:
            fail(failures, f"dialogue auto-compact replay missing snippet: {snippet}")
    if "yield_missing_tool_result_blocks(&prepared.messages)" in dialogue:
        fail(failures, "dialogue must not repair tool results against stale prepared messages")

    forbidden_service_markers = [
        "In production, would call compact_conversation here.",
        "For now, return not compacted (placeholder for integration).",
    ]
    for snippet in forbidden_service_markers:
        if snippet in service_auto:
            fail(failures, f"service auto-compact still has placeholder: {snippet}")
    for snippet in [
        "let compact_result = compact_conversation(messages, \"Read\").await;",
        "conversation_result_to_compaction_result(",
        "\"compact_metadata\"",
        "\"trigger\": \"auto\"",
        "auto_compact_invokes_compact_conversation_when_threshold_is_reached",
        "auto_compact_circuit_breaker_skips_before_compacting",
    ]:
        if snippet not in service_auto:
            fail(failures, f"service auto-compact missing snippet: {snippet}")

    if "wave_w79_auto_compact_query_replay_smoke" not in run_all:
        fail(failures, "run_all_smoke.sh must register W79")

    print("=== W79 auto-compact query replay smoke ===")
    print(f"context: {CONTEXT.relative_to(ROOT)}")
    print(f"dialogue: {DIALOGUE.relative_to(ROOT)}")
    print(f"service auto-compact: {SERVICE_AUTO.relative_to(ROOT)}")
    if failures:
        print("\nFAILURES:")
        for failure in failures:
            print(f" - {failure}")
        return 1
    print("PASS: auto-compact replay uses compacted context and service path is wired")
    return 0


if __name__ == "__main__":
    sys.exit(main())
