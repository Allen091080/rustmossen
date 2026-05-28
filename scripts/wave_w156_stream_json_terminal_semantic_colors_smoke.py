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
        "terminal_semantic_style_for_line",
        "terminal_semantic_color",
        '"semanticStyle"',
        "styled_line_count",
        "style_reset_count",
        '"semanticColors"',
        '"plainTextFallback"',
        '"resetAfterLine"',
        '"semanticStyleResets"',
        "draw_scheduler_attaches_semantic_styles_to_region_lines",
        "draw_executor_applies_semantic_colors_and_resets_after_each_styled_line",
    ):
        require(renderer, token, f"semantic color token {token}")

    for token in (
        '"terminal_semantic_colors"',
        '"terminal_color_plain_fallback"',
        '"terminal_style_reset_after_line"',
    ):
        require(structured_io, token, f"status semantic color metadata {token}")

    require(
        run_all,
        "wave_w156_stream_json_terminal_semantic_colors_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal semantic colors",
        "phase note",
    )

    print("wave_w156_stream_json_terminal_semantic_colors_smoke: ok")


if __name__ == "__main__":
    main()
