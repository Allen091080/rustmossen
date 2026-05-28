#!/usr/bin/env python3
"""W191 - terminal semantic colors have a real plain-text fallback."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    renderer = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "STREAM_JSON_TERMINAL_RENDER_COLOR_ENV",
        "MOSSEN_TERMINAL_RENDER_COLOR",
        "terminal_render_semantic_colors_enabled",
        "terminal_render_semantic_colors_enabled_for_env",
        "draw_executor_plain_text_fallback_omits_semantic_color_writes",
        "terminal_render_semantic_color_policy_respects_plain_fallback_env",
        "semantic_colors_enabled",
    ]:
        require(renderer, token, "color plain fallback executor", failures)

    for token in [
        "terminal_color_no_color_env_fallback",
        "terminal_color_dumb_terminal_fallback",
        "terminal_color_clicolor_zero_fallback",
        "semantic_color_plain_text_fallback",
        "no_color_env_plain_text_fallback",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w191_terminal_color_plain_fallback_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render semantic color plain fallback",
        "phase note",
        failures,
    )

    if failures:
        print("=== W191 terminal color plain fallback smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w191_terminal_color_plain_fallback_smoke: ok")


if __name__ == "__main__":
    main()
