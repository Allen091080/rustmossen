#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAIN = ROOT / "crates/mossen-cli/src/main.rs"
REPL = ROOT / "crates/mossen-cli/src/repl.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
BRIDGE = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    main_rs = MAIN.read_text()
    repl = REPL.read_text()
    structured_io = STRUCTURED_IO.read_text()
    bridge = BRIDGE.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    require(main_rs, "mod stream_json_render_events;", "module registration")

    for token in (
        "STREAM_JSON_RENDER_EVENT_SCHEMA_VERSION",
        "STREAM_JSON_RENDER_EVENT_TYPE",
        "STREAM_JSON_RENDER_EVENT_THROTTLE_MS",
        "stream_json_render_events_for_sdk_message",
        "render_events_for_sdk_message",
        '"type"',
        '"render_event"',
        '"schemaVersion"',
        '"refresh"',
        '"history"',
        '"payload"',
        "serializes_stream_text_delta_as_throttled_render_event",
        "serializes_result_as_turn_and_final_summary_events",
    ):
        require(bridge, token, f"render bridge token {token}")

    for token in (
        "StreamJsonRenderEventEmitter",
        "emit_stream_items_for_sdk_message(&msg)",
        "emit_stream_items_for_sdk_message(&fallback)",
        "StdoutMessage::StreamEvent(item)",
    ):
        require(repl, token, f"repl render event emission {token}")

    for token in (
        '"render"',
        '"event_stream"',
        '"event_type"',
        '"schema_version"',
        '"raw_sdk_messages"',
        '"throttle_ms"',
    ):
        require(structured_io, token, f"status render metadata {token}")

    require(
        run_all,
        "wave_w134_stream_json_render_event_bridge_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json render event bridge",
        "phase note",
    )

    print("wave_w134_stream_json_render_event_bridge_smoke: ok")


if __name__ == "__main__":
    main()
