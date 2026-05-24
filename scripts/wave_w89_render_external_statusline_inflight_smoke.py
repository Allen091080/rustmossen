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
    active_animation = app.split("fn has_active_render_animation(&self) -> bool {", 1)[1].split(
        "\n    fn active_render_frame_interval",
        1,
    )[0]

    forbid(
        active_animation,
        "external_statusline_in_flight",
        "external statusline in-flight active animation driver",
    )
    require(
        app,
        "matches!(self.active_modal, ActiveModal::DebugConfig(_))",
        "debug-only external statusline in-flight fingerprint",
    )
    require(
        app,
        "external_statusline_inflight_does_not_drive_invisible_frames",
        "external statusline in-flight frame regression",
    )
    require(
        app,
        "starting an invisible external statusline command should not dirty the main surface",
        "invisible statusline dirty assertion",
    )
    require(
        app,
        "external statusline in-flight state should not drive the active render loop",
        "active animation assertion",
    )
    require(
        run_all,
        "wave_w89_render_external_statusline_inflight_smoke.py",
        "run_all registration",
    )

    print("wave_w89_render_external_statusline_inflight_smoke: ok")


if __name__ == "__main__":
    main()
