#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    types = read("crates/mossen-agent/src/types.rs")
    dialogue = read("crates/mossen-agent/src/dialogue.rs")
    render_events = read("crates/mossen-tui/src/render_events.rs")
    app = read("crates/mossen-tui/src/app.rs")
    stream_json = read("crates/mossen-cli/src/stream_json_render_events.rs")
    run_all = read("scripts/run_all_smoke.sh")
    phase = read("phases/03g-rendering-product-grade-plan.md")

    for needle in [
        '#[serde(rename = "compact_request_status")]',
        "CompactRequestStatus {",
        "pub enum CompactRequestStatus",
        "CompactRequestStatus::TimedOut => \"timed_out\"",
        "CompactRequestStatus::DryRun => \"dry_run\"",
        "CompactRequestStatus::Completed => \"completed\"",
        "CompactRequestStatus::Skipped => \"skipped\"",
        "CompactRequestStatus::Failed => \"failed\"",
    ]:
        require(types, needle, "agent compact status protocol")

    for needle in [
        "async fn emit_compact_request_status",
        "fn compact_request_status_reason",
        "CompactRequestStatus::TimedOut",
        "CompactRequestStatus::DryRun",
        "CompactRequestStatus::Failed",
        "CompactRequestStatus::Skipped",
        "CompactRequestStatus::Completed",
        "pending_compact_request_compacts_state_and_emits_boundary",
        "pending_compact_request_dry_run_does_not_mutate_or_emit_boundary",
        "pending_compact_request_skipped_emits_status_event",
    ]:
        require(dialogue, needle, "dialogue safe-point compact status emission")

    for needle in [
        "RenderEventKind::CompactRequestStatus",
        "compact_request_status_maps_to_visible_render_event",
    ]:
        require(render_events, needle, "TUI compact status render event")

    for needle in [
        "SdkMessage::CompactRequestStatus",
        "request {}",
        "RenderActivity::CompactStatus",
        "Compact status",
    ]:
        require(app, needle, "TUI compact status state/activity handling")

    for needle in [
        "SdkMessage::CompactRequestStatus { .. } => \"compact_request_status\"",
        "RenderEventKind::CompactRequestStatus { .. } => \"compact_request_status\"",
        '"compact_request_status" =>',
        "serializes_compact_request_status_as_immediate_render_event",
    ]:
        require(stream_json, needle, "stream-json compact status bridge")

    require(
        run_all,
        "wave_w225_compact_safe_point_status_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Compact safe-point status events",
        "phase note",
    )

    print("wave_w225_compact_safe_point_status_smoke: ok")


if __name__ == "__main__":
    main()
