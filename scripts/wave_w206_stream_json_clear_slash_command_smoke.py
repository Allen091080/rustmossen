#!/usr/bin/env python3
"""W206 - stream-json /clear slash command queues safe-point clearing."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    dialogue = (ROOT / "crates/mossen-agent/src/dialogue.rs").read_text()
    pending_clear = (
        ROOT / "crates/mossen-agent/src/services/root/pending_clear_request.rs"
    ).read_text()
    agent_types = (ROOT / "crates/mossen-agent/src/types.rs").read_text()
    render_events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    caps = (
        ROOT / "crates/mossen-agent/src/services/root/slash_command_capabilities.rs"
    ).read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        '"clear" => match build_clear_slash_response(request_id.clone(), &args)',
        "fn build_clear_slash_response",
        "fn pending_clear_status",
        '"pending_clear_request"',
        "slash_command_clear_preview_and_confirm_queue_request",
    ]:
        require(structured, token, "clear slash handler", failures)

    for token in [
        "pub const CLEAR_REQUEST_TIMEOUT",
        "pub struct PendingClearRequest",
        "enqueue_pending_clear_request",
        "dequeue_pending_clear_request",
        "clear_pending_clear_request",
    ]:
        require(pending_clear, token, "pending clear buffer", failures)

    for token in [
        "execute_pending_clear_request(&mut state, tx).await",
        "async fn execute_pending_clear_request",
        "async fn execute_clear_request_at_safe_point",
        "pending_clear_request_clears_state_and_emits_event",
        "post_compact_cleanup::run_post_compact_cleanup(Some(\"sdk\"))",
    ]:
        require(dialogue, token, "dialogue safe-point execution", failures)

    require(agent_types, "ConversationCleared", "sdk event", failures)
    require(render_events, "conversation_cleared", "render event mapping", failures)
    require(
        caps,
        "structured_io.rs:slash_command/clear",
        "capability source",
        failures,
    )
    require(
        run_all,
        "wave_w206_stream_json_clear_slash_command_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json /clear slash command bridge",
        "phase note",
        failures,
    )

    if failures:
        print("=== W206 stream-json /clear slash command smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w206_stream_json_clear_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
