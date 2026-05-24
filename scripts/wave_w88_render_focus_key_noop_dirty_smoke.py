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

    require(app, "let transcript_focus_key = !self.active_modal.is_open()", "focus-key guard")
    require(
        app,
        "before_transcript_key_state",
        "visible transcript key fingerprint",
    )
    require(
        app,
        "no_op_transcript_focus_keys_do_not_dirty_frame",
        "no-op focus key regression",
    )
    require(
        app,
        "focus Up on an empty transcript should not schedule a redraw",
        "empty transcript focus assertion",
    )
    require(
        app,
        "focus Down that keeps the same visible transcript state should not schedule a redraw",
        "single-message focus assertion",
    )
    require(
        run_all,
        "wave_w88_render_focus_key_noop_dirty_smoke.py",
        "run_all registration",
    )

    print("wave_w88_render_focus_key_noop_dirty_smoke: ok")


if __name__ == "__main__":
    main()
