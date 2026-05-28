#!/usr/bin/env python3
"""W193 - terminal renderer has a real ASCII glyph fallback mode."""

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
        "STREAM_JSON_TERMINAL_RENDER_UNICODE_ENV",
        "MOSSEN_TERMINAL_RENDER_UNICODE",
        "terminal_render_unicode_enabled",
        "terminal_render_unicode_enabled_for_env",
        "terminal_ascii_fallback_grapheme",
        "with_terminal_capabilities",
        "ascii_fallback_count",
        "terminal_bounded_line_ascii_fallback_replaces_fancy_glyphs",
        "draw_executor_ascii_fallback_omits_unicode_output",
    ]:
        require(renderer, token, "ascii fallback renderer", failures)

    for token in [
        "terminal_ascii_glyph_fallback",
        "terminal_unicode_ascii_mode_policy",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w193_terminal_ascii_fallback_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render ASCII glyph fallback",
        "phase note",
        failures,
    )

    if failures:
        print("=== W193 terminal ascii fallback smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w193_terminal_ascii_fallback_smoke: ok")


if __name__ == "__main__":
    main()
