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
    render_lifecycle = read("crates/mossen-tui/src/render_lifecycle.rs")
    stream_json = read("crates/mossen-cli/src/stream_json_render_events.rs")
    run_all = read("scripts/run_all_smoke.sh")
    phase = read("phases/03g-rendering-product-grade-plan.md")

    for needle in [
        '#[serde(rename = "clear_request_status")]',
        "ClearRequestStatus {",
        "pub enum ClearRequestStatus",
        "ClearRequestStatus::TimedOut => \"timed_out\"",
        "ClearRequestStatus::DryRun => \"dry_run\"",
        "ClearRequestStatus::Completed => \"completed\"",
    ]:
        require(types, needle, "agent clear status protocol")

    for needle in [
        "async fn emit_clear_request_status",
        "ClearRequestStatus::TimedOut",
        "ClearRequestStatus::DryRun",
        "ClearRequestStatus::Completed",
        "pending_clear_request_clears_state_and_emits_event",
        "pending_clear_request_dry_run_emits_status_event",
    ]:
        require(dialogue, needle, "dialogue safe-point clear status emission")

    for needle in [
        "RenderEventKind::ClearRequestStatus",
        "clear_request_status_maps_to_visible_render_event",
    ]:
        require(render_events, needle, "TUI clear status render event")

    for needle in [
        "SdkMessage::ClearRequestStatus",
        "RenderActivity::ClearStatus",
        "Clear status",
    ]:
        require(app, needle, "TUI clear status activity handling")

    for needle in [
        "RawEngineEventKind::ClearRequestStatus",
        '"clear_request_status"',
    ]:
        require(render_lifecycle, needle, "TUI raw lifecycle clear status")

    for needle in [
        "SdkMessage::ClearRequestStatus { .. } => \"clear_request_status\"",
        "RenderEventKind::ClearRequestStatus { .. } => \"clear_request_status\"",
        '"clear_request_status" =>',
        "serializes_clear_request_status_as_immediate_render_event",
    ]:
        require(stream_json, needle, "stream-json clear status bridge")

    require(
        run_all,
        "wave_w226_clear_safe_point_status_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Clear safe-point status events",
        "phase note",
    )

    print("wave_w226_clear_safe_point_status_smoke: ok")


if __name__ == "__main__":
    main()
