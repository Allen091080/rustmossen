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
        "terminal_draw_soft_wrapped_lines",
        "scrollback_wrapped_line_count",
        '"wrapLongLines": true',
        '"wrapMode": "soft_viewport_columns"',
        'assert!(!ansi.contains("..."))',
    ):
        require(renderer, token, f"scrollback soft-wrap token {token}")

    for token in (
        '"terminal_scrollback_soft_wrap": true',
        '"terminal_scrollback_soft_wrap"',
    ):
        require(structured_io, token, f"status soft-wrap metadata {token}")

    require(
        run_all,
        "wave_w153_stream_json_terminal_scrollback_soft_wrap_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal scrollback soft wrap",
        "phase note",
    )

    print("wave_w153_stream_json_terminal_scrollback_soft_wrap_smoke: ok")


if __name__ == "__main__":
    main()
