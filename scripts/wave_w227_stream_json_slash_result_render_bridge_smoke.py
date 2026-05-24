#!/usr/bin/env python3
"""W227 - stream-json slash command results enter the terminal render stream."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    render_events = (ROOT / "crates/mossen-tui/src/render_events.rs").read_text()
    state = (ROOT / "crates/mossen-tui/src/state.rs").read_text()
    app = (ROOT / "crates/mossen-tui/src/app.rs").read_text()
    render_model = (ROOT / "crates/mossen-tui/src/render_model.rs").read_text()
    bridge = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []

    for token in [
        "SlashCommandResult",
        'RenderEventKind::SlashCommandResult { .. }',
        "RenderHistoryPolicy::FreezeHistory",
    ]:
        require(render_events, token, "render event contract", failures)

    for token in [
        "RenderActivity::SlashCommand",
        "Slash command",
        "process_row_from_activity",
    ]:
        require(app, token, "TUI activity wiring", failures)
    require(state, "SlashCommand", "TUI activity state", failures)
    require(render_model, '"slash_command_result"', "timeline mapping", failures)

    for token in [
        "emit_slash_command_result_items",
        "slash_command_result_summary",
        '"slash_command_result"',
        "emits_slash_command_result_as_terminal_render_items",
    ]:
        require(bridge, token, "stream-json render bridge", failures)

    for token in [
        "new_with_render_event_emitter",
        "emit_slash_command_render_items",
        "slash_command_response_emits_render_items_when_renderer_is_attached",
        "StreamJsonRenderEventEmitter",
    ]:
        require(structured, token, "StructuredIO render response bridge", failures)

    for token in [
        "StructuredIO::new_with_render_event_emitter",
        "render_event_emitter.lock().await",
    ]:
        require(repl, token, "shared stream-json emitter", failures)

    require(
        run_all,
        "wave_w227_stream_json_slash_result_render_bridge_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json slash result render bridge",
        "phase note",
        failures,
    )

    if failures:
        print("=== W227 stream-json slash result render bridge smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w227_stream_json_slash_result_render_bridge_smoke: ok")


if __name__ == "__main__":
    main()
