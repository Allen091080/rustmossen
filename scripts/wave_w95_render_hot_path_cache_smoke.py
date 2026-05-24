#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
CONTRACT = ROOT / "crates/mossen-tui/tests/render_contract.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    contract = CONTRACT.read_text()
    run_all = RUN_ALL.read_text()

    require(app, '"Transcript Cache"', "debug renderer transcript cache row")
    require(app, '"Frame Scheduler"', "debug renderer frame scheduler row")
    require(app, "render_transcript_cache_stats", "transcript cache diagnostics source")
    require(app, "render_frame_scheduler_stats", "frame scheduler diagnostics source")
    require(
        app,
        "repeated_long_session_frames_reuse_transcript_cache_for_visible_state_changes",
        "long-session repeated-frame cache regression",
    )
    require(
        app,
        "visible status changes must not rebuild the long transcript model",
        "status-only cache-hit assertion",
    )
    require(
        app,
        "prompt repaint must keep using the cached long transcript model",
        "prompt-only cache-hit assertion",
    )
    require(
        contract,
        '"Transcript Cache"',
        "debug config product contract transcript cache assertion",
    )
    require(
        contract,
        '"Frame Scheduler"',
        "debug config product contract frame scheduler assertion",
    )
    require(
        run_all,
        "wave_w95_render_hot_path_cache_smoke.py",
        "run_all registration",
    )

    print("wave_w95_render_hot_path_cache_smoke: ok")


if __name__ == "__main__":
    main()
