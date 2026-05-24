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
        "STREAM_JSON_RENDER_SNAPSHOT_SCHEMA_VERSION",
        "STREAM_JSON_RENDER_SNAPSHOT_TYPE",
        "pub struct StreamJsonRenderStreamState",
        "apply_render_event_value",
        "snapshot_value",
        "render_snapshot_activity",
        '"render_snapshot"',
        '"lastSequence"',
        '"appliedCount"',
        '"ignoredStaleCount"',
        '"pendingThrottledRender"',
        '"needsImmediateRender"',
        '"preserveScrollOnUpdateActive"',
        "emit_stream_items_for_sdk_message",
        "emits_render_snapshot_after_each_source_message",
        "stream_state_reduces_events_and_ignores_stale_duplicates",
    ):
        require(bridge, token, f"snapshot reducer token {token}")

    for token in (
        "emit_stream_items_for_sdk_message(&msg)",
        "emit_stream_items_for_sdk_message(&fallback)",
        "StdoutMessage::StreamEvent(item)",
    ):
        require(repl, token, f"repl snapshot emission {token}")

    for token in (
        '"snapshot_stream"',
        '"snapshot_type"',
        '"snapshot_schema_version"',
        'body["runtime"]["render"]["snapshot_stream"], true',
        'body["runtime"]["render"]["snapshot_type"]',
    ):
        require(structured_io, token, f"status snapshot metadata {token}")

    require(
        run_all,
        "wave_w136_stream_json_render_snapshot_reducer_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json render snapshot reducer",
        "phase note",
    )

    print("wave_w136_stream_json_render_snapshot_reducer_smoke: ok")


if __name__ == "__main__":
    main()
