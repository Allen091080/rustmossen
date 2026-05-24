#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDERER = ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    renderer = RENDERER.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "pub struct StreamJsonTerminalDrawRuntime",
        "pub struct StreamJsonTerminalDrawRuntimeReport",
        "pending_draw_plan: Option<Value>",
        "last_flush_at_ms: Option<u64>",
        "next_flush_due_ms: Option<u64>",
        "manual_scroll_active: bool",
        "pub fn submit_draw_plan_at",
        "pub fn flush_pending_at",
        "pub fn set_manual_scroll_active",
        "pub fn set_viewport",
        "coalesced_until_throttle_deadline",
        "throttle_deadline_not_reached",
        "manual_scroll_preserved",
        "draw_runtime_coalesces_throttled_plans_until_deadline",
        "draw_runtime_applies_pending_plan_with_latest_resize_viewport",
        "draw_runtime_holds_active_updates_while_manual_scroll_is_active",
    ):
        require(renderer, token, f"draw runtime token {token}")

    for token in (
        '"draw_runtime_queue": true',
        '"draw_runtime_coalescing": true',
        '"draw_runtime_resize_aware": true',
        '"draw_runtime_manual_scroll_hold": true',
        '"coalesced_runtime_queue": true',
        '"throttle_deadline_flush": true',
        '"resize_before_pending_flush": true',
        '"manual_scroll_preserves_active_update": true',
    ):
        require(structured_io, token, f"status draw runtime metadata {token}")

    require(
        run_all,
        "wave_w142_stream_json_terminal_draw_runtime_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal draw runtime queue",
        "phase note",
    )

    print("wave_w142_stream_json_terminal_draw_runtime_smoke: ok")


if __name__ == "__main__":
    main()
