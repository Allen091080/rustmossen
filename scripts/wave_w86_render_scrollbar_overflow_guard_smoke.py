#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    run_all = RUN_ALL.read_text()

    require(
        app,
        "let full_width_total_rows = self.message_total_rows(surface, content_area.width);",
        "full-width overflow decision",
    )
    require(
        app,
        "if full_width_total_rows <= content_area.height as usize",
        "no self-induced rail guard",
    )
    require(
        app,
        "transcript_scrollbar_does_not_create_its_own_overflow",
        "scrollbar overflow regression",
    )
    require(
        app,
        "the scrollbar rail must not appear only because reserving the rail would make text wrap",
        "self-induced overflow assertion",
    )
    require(
        run_all,
        "wave_w86_render_scrollbar_overflow_guard_smoke.py",
        "run_all registration",
    )

    print("wave_w86_render_scrollbar_overflow_guard_smoke: ok")


if __name__ == "__main__":
    main()
