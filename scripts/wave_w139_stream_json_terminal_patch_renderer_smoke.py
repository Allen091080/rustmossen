#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAIN = ROOT / "crates/mossen-cli/src/main.rs"
BRIDGE = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
RENDERER = ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    main_rs = MAIN.read_text()
    bridge = BRIDGE.read_text()
    renderer = RENDERER.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    require(main_rs, "mod stream_json_terminal_renderer;", "module registration")

    for token in (
        "StreamJsonTerminalPatchRenderer",
        "terminal_patch_renderer",
        "render_frame_value(&frame)",
        "STREAM_JSON_RENDER_PATCH_TYPE",
        "emits_terminal_patch_after_frame",
    ):
        require(bridge, token, f"bridge patch token {token}")

    for token in (
        "STREAM_JSON_RENDER_PATCH_SCHEMA_VERSION",
        "STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS",
        "pub struct StreamJsonTerminalPatchRenderer",
        "pub fn render_frame_value",
        '"replace_region"',
        '"skipReason"',
        '"preservePrompt"',
        '"ansiSafeLines"',
        "renderer_skips_duplicate_frame_hash",
        "renderer_strips_controls_and_truncates_pathological_lines",
    ):
        require(renderer, token, f"renderer token {token}")

    for token in (
        '"patch_stream": true',
        '"patch_type": STREAM_JSON_RENDER_PATCH_TYPE',
        '"patch_schema_version": STREAM_JSON_RENDER_PATCH_SCHEMA_VERSION',
        '"patch_operations": true',
        '"skip_duplicate_frames": true',
        '"ansi_safe_lines": true',
        '"preserve_prompt_cursor": true',
    ):
        require(structured_io, token, f"status patch metadata {token}")

    require(
        run_all,
        "wave_w139_stream_json_terminal_patch_renderer_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal patch renderer",
        "phase note",
    )

    print("wave_w139_stream_json_terminal_patch_renderer_smoke: ok")


if __name__ == "__main__":
    main()
