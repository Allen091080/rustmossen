#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    prompt_input = read("crates/mossen-tui/src/widgets/prompt_input.rs")
    app = read("crates/mossen-tui/src/app.rs")
    stream_json = read("crates/mossen-cli/src/stream_json_render_events.rs")
    run_all = read("scripts/run_all_smoke.sh")
    phase = read("phases/03g-rendering-product-grade-plan.md")

    for needle in [
        "const PROMPT_COMPOSER_HEIGHT: u16 = 3",
        "fn render_input_box",
        ".borders(Borders::ALL)",
        ".border_set(glyphs.border)",
        "active_prompt_renders_as_visible_composer_box",
    ]:
        require(prompt_input, needle, "visible composer input box")

    for needle in [
        "fn final_summary_should_record",
        "if !final_summary_should_record(&model)",
        "successful_text_only_result_skips_default_final_summary_noise",
        "RenderEventKind::FinalSummaryRecorded",
    ]:
        require(app, needle, "default final summary suppression")

    for needle in [
        "fn should_emit_final_summary_for_result",
        "fn has_terminal_work_activity",
        "serializes_success_result_without_empty_final_summary_event",
        "serializes_final_summary_after_terminal_work_activity",
    ]:
        require(stream_json, needle, "stream-json final summary activity gate")

    require(
        run_all,
        "wave_w324_tui_composer_final_summary_noise_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-24 TUI composer visibility and final-summary noise gate",
        "phase note",
    )

    print("wave_w324_tui_composer_final_summary_noise_smoke: ok")


if __name__ == "__main__":
    main()
