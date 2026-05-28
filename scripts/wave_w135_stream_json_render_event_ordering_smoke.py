#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BRIDGE = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
REPL = ROOT / "crates/mossen-cli/src/repl.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    bridge = BRIDGE.read_text()
    repl = REPL.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "STREAM_JSON_RENDER_EVENT_SCHEMA_VERSION: u32 = 2",
        "pub struct StreamJsonRenderEventEmitter",
        "next_event_sequence",
        "next_source_message_sequence",
        "emit_for_sdk_message",
        '"sequence"',
        '"stream"',
        '"eventSequence"',
        '"sourceMessageSequence"',
        '"sourceMessageType"',
        '"eventIndexInSource"',
        '"emittedAtMs"',
        "sdk_message_type_key",
        "emitter_assigns_monotonic_ordering_across_messages",
    ):
        require(bridge, token, f"render ordering token {token}")

    for token in (
        "StreamJsonRenderEventEmitter::new()",
        "render_event_emitter.emit_stream_items_for_sdk_message(&msg)",
        "render_event_emitter.emit_stream_items_for_sdk_message(&fallback)",
    ):
        require(repl, token, f"repl stateful emitter token {token}")

    for token in (
        '"ordering"',
        '"monotonic_event_sequence"',
        '"source_message_sequence"',
        '"event_index_in_source"',
        '"emitted_at_ms"',
        'body["runtime"]["render"]["schema_version"], 2',
    ):
        require(structured_io, token, f"status ordering metadata {token}")

    require(
        run_all,
        "wave_w135_stream_json_render_event_ordering_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json render event ordering",
        "phase note",
    )

    print("wave_w135_stream_json_render_event_ordering_smoke: ok")


if __name__ == "__main__":
    main()
