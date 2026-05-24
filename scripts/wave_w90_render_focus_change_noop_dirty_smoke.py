#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def forbid(text: str, needle: str, label: str) -> None:
    if needle in text:
        raise SystemExit(f"forbidden {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    run_all = RUN_ALL.read_text()
    focus_branch = app.split("AppEvent::FocusChange(focused) => {", 1)[1].split(
        "\n            AppEvent::Quit",
        1,
    )[0]
    fingerprint = app.split("fn render_tick_fingerprint(&self) -> u64 {", 1)[1].split(
        "\n    fn render_session_snapshot",
        1,
    )[0]

    require(
        focus_branch,
        "let before = self.render_tick_fingerprint();",
        "focus-change visible-state fingerprint",
    )
    require(
        focus_branch,
        "if self.render_tick_fingerprint() != before",
        "focus-change dirty guard",
    )
    forbid(
        fingerprint,
        "notification_fired",
        "notification latch in visible render fingerprint",
    )
    require(
        app,
        "focus_change_updates_notification_latch_without_dirty_frame",
        "focus-change no-op dirty regression",
    )
    require(
        app,
        "focus gained should still reset the notification latch",
        "notification latch assertion",
    )
    require(
        app,
        "hidden focus-state changes should not schedule a redraw",
        "hidden focus dirty assertion",
    )
    require(
        run_all,
        "wave_w90_render_focus_change_noop_dirty_smoke.py",
        "run_all registration",
    )

    print("wave_w90_render_focus_change_noop_dirty_smoke: ok")


if __name__ == "__main__":
    main()
