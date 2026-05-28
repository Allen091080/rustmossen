#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
STATE = ROOT / "crates/mossen-tui/src/state.rs"
EVENTS = ROOT / "crates/mossen-tui/src/render_events.rs"
MODEL = ROOT / "crates/mossen-tui/src/render_model.rs"
CONTRACT = ROOT / "crates/mossen-tui/tests/render_contract.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    state = STATE.read_text()
    events = EVENTS.read_text()
    model = MODEL.read_text()
    contract = CONTRACT.read_text()
    run_all = RUN_ALL.read_text()

    for needle in [
        "completed_count: usize",
        "active_count: usize",
        "pending_count: usize",
        "blocked_count: usize",
    ]:
        require(events, needle, f"plan event count field {needle}")
        require(state, needle, f"render activity count field {needle}")

    require(events, "fn plan_status_counts", "plan status counter")
    require(model, "fn plan_event_summary_line", "timeline plan progress summary")
    require(app, "fn plan_activity_progress_summary", "activity panel plan progress summary")
    require(
        app,
        "render_surface_carries_plan_progress_counts_in_active_panel",
        "plan activity panel regression",
    )
    require(
        contract,
        "app_render_contract_plan_activity_panel_shows_progress_counts",
        "plan activity product contract",
    )
    require(contract, "1 blocked", "blocked plan progress contract")
    require(
        run_all,
        "wave_w92_render_plan_activity_progress_smoke.py",
        "run_all registration",
    )

    print("wave_w92_render_plan_activity_progress_smoke: ok")


if __name__ == "__main__":
    main()
