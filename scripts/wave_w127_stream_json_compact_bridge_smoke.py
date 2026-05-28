#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
DIALOGUE = ROOT / "crates/mossen-agent/src/dialogue.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    structured_io = STRUCTURED_IO.read_text()
    dialogue = DIALOGUE.read_text()
    run_all = RUN_ALL.read_text()

    require(
        structured_io,
        "handle_compact_conversation_control_request",
        "compact_conversation control_request handler",
    )
    require(
        structured_io,
        "enqueue_pending_compact_request(",
        "compact_conversation enqueues pending request",
    )
    require(
        structured_io,
        '"status": status',
        "compact_conversation queued/blocked response payload",
    )
    require(
        structured_io,
        "ControlResponse(SDKControlResponse)",
        "StructuredIO can emit control_response",
    )
    require(
        structured_io,
        "compact_conversation_control_request_enqueues_and_responds",
        "StructuredIO enqueue test coverage",
    )
    require(
        dialogue,
        "dequeue_pending_compact_request",
        "dialogue consumes pending compact requests",
    )
    require(
        dialogue,
        "execute_pending_compact_request(&mut state, tx).await",
        "dialogue loop safe point invokes compact request executor",
    )
    require(
        dialogue,
        "compact_conversation_with_options(&state.messages, \"Read\", options).await",
        "dialogue executes real compact conversation",
    )
    require(
        dialogue,
        "prepend_compact_boundary_to_messages(",
        "stream-json compact writes compact boundary into model context",
    )
    require(
        dialogue,
        "post_compact_cleanup::run_post_compact_cleanup(Some(\"sdk\"))",
        "stream-json compact runs post compact cleanup",
    )
    require(
        dialogue,
        "pending_compact_request_compacts_state_and_emits_boundary",
        "dialogue compact execution test coverage",
    )
    require(
        dialogue,
        "pending_compact_request_dry_run_does_not_mutate_or_emit_boundary",
        "dialogue dry-run test coverage",
    )
    require(
        run_all,
        "wave_w127_stream_json_compact_bridge_smoke.py",
        "run_all registration",
    )

    print("wave_w127_stream_json_compact_bridge_smoke: ok")


if __name__ == "__main__":
    main()
