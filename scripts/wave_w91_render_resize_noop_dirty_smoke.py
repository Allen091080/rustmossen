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
    resize_branch = app.split("AppEvent::Resize { width, height } => {", 1)[1].split(
        "\n            AppEvent::Tick",
        1,
    )[0]

    require(
        resize_branch,
        "let before_resize_state = (",
        "resize visible-state fingerprint",
    )
    require(
        resize_branch,
        "let after_resize_state = (",
        "resize post-state fingerprint",
    )
    require(
        resize_branch,
        "if before_resize_state != after_resize_state",
        "resize no-op dirty guard",
    )
    require(
        app,
        "same_size_resize_without_visible_state_change_does_not_dirty_frame",
        "same-size resize regression",
    )
    require(
        app,
        "same-size resize with unchanged scroll state should not redraw",
        "no-op resize assertion",
    )
    require(
        app,
        "same terminal dimensions must still redraw when resize synchronizes viewport state",
        "viewport synchronization assertion",
    )
    require(
        run_all,
        "wave_w91_render_resize_noop_dirty_smoke.py",
        "run_all registration",
    )

    print("wave_w91_render_resize_noop_dirty_smoke: ok")


if __name__ == "__main__":
    main()
