#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "crates/mossen-cli/src/cli.rs"
MAIN = ROOT / "crates/mossen-cli/src/main.rs"
REPL = ROOT / "crates/mossen-cli/src/repl.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    cli = CLI.read_text()
    main = MAIN.read_text()
    repl = REPL.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "Terminal,",
        "本进程直接渲染终端 UI",
    ):
        require(cli, token, f"CLI terminal emit token {token}")

    for token in (
        "run_oneshot_terminal_render",
        "EmitFormat::Terminal",
    ):
        require(main, token, f"main terminal routing token {token}")

    for token in (
        "pub async fn run_oneshot_terminal_render",
        "StreamJsonTerminalDrawRuntime::for_current_terminal",
        "STREAM_JSON_RENDER_DRAW_PLAN_TYPE",
        "terminal_render_handle_sdk_message",
        "terminal_render_next_flush_due",
        "flush_pending_at",
        "submit_draw_plan_at",
        "run_oneshot_stream_json",
    ):
        require(repl, token, f"terminal frontend token {token}")

    for token in (
        '"terminal_frontend": true',
        '"terminal_frontend_emit": "terminal"',
        '"terminal_frontend_transport_isolated": true',
        '"terminal_frontend_log_isolated": true',
        '"terminal_scrollback_transcript": true',
        '"terminal_approval_widget": true',
        '"terminal_command_widget": true',
        '"terminal_diff_widget": true',
        '"terminal_error_widget": true',
        '"terminal_final_summary_widget": true',
        '"terminal_viewport_collision_guard": true',
        '"terminal_retired_region_clear"',
        '"draw_runtime_manual_scroll_critical_bypass": true',
        '"terminal_frontend_emit_mode": true',
        '"ndjson_ansi_isolation": true',
        '"terminal_frontend_log_isolation": true',
        '"terminal_scrollback_transcript_commit": true',
        '"terminal_scrollback_append_once": true',
        '"independent_approval_region": true',
        '"approval_blocks_active_log": true',
        '"approval_draw_plan_blocking_region": true',
        '"independent_command_region": true',
        '"command_output_summary_only": true',
        '"independent_diff_region": true',
        '"diff_collapsed_by_default": true',
        '"independent_error_region": true',
        '"layered_error_region": true',
        '"independent_final_summary_region": true',
        '"final_summary_terminal_region": true',
        '"terminal_top_bottom_collision_guard"',
        '"terminal_independent_widget_suppresses_duplicate_active"',
        '"terminal_retired_region_clear"',
        '"manual_scroll_critical_draw_bypass"',
    ):
        require(structured_io, token, f"status terminal frontend metadata {token}")

    require(
        run_all,
        "wave_w143_stream_json_terminal_frontend_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal frontend emit mode",
        "phase note",
    )

    print("wave_w143_stream_json_terminal_frontend_smoke: ok")


if __name__ == "__main__":
    main()
