#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATE = ROOT / "crates/mossen-tui/src/state.rs"
APP = ROOT / "crates/mossen-tui/src/app.rs"
CONTRACT = ROOT / "crates/mossen-tui/tests/render_contract.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    state = STATE.read_text()
    app = APP.read_text()
    contract = CONTRACT.read_text()
    run_all = RUN_ALL.read_text()

    require(state, "Self::Plan {", "plan activity status-line branch")
    require(state, "blocked_count", "plan activity blocked count")
    require(app, "process_row_from_activity", "process activity row bridge")
    require(app, "footer_render_model", "status overview activity source")
    require(
        contract,
        "app_render_contract_process_and_status_show_plan_progress_counts",
        "plan process/status product contract",
    )
    require(contract, "Process Status", "process modal assertion")
    require(contract, "Plan activity", "process plan activity assertion")
    require(contract, "plan: 4 steps", "plan status-line assertion")
    require(contract, "1 blocked", "blocked count assertion")
    require(contract, "Status", "status overview assertion")
    require(
        run_all,
        "wave_w94_render_plan_process_status_smoke.py",
        "run_all registration",
    )

    print("wave_w94_render_plan_process_status_smoke: ok")


if __name__ == "__main__":
    main()
