#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODEL = ROOT / "crates/mossen-tui/src/render_model.rs"
CONTRACT = ROOT / "crates/mossen-tui/tests/render_contract.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    model = MODEL.read_text()
    contract = CONTRACT.read_text()
    run_all = RUN_ALL.read_text()

    require(model, "fn plan_event_summary_line", "plan timeline summary helper")
    require(
        model,
        "render_timeline_preserves_plan_progress_counts",
        "plan timeline model regression",
    )
    require(model, "Some(render_timeline_detail([", "selected plan timeline detail")
    require(
        contract,
        "app_render_contract_timeline_modal_shows_plan_progress_counts",
        "plan timeline product contract",
    )
    require(contract, "plan updated: 4 step(s)", "plan timeline total count")
    require(contract, "1 blocked", "plan timeline blocked count")
    require(contract, "detail: plan updated", "plan timeline selected detail")
    require(contract, "\"status\"", "raw TodoWrite key leak guard")
    require(
        run_all,
        "wave_w93_render_plan_timeline_progress_smoke.py",
        "run_all registration",
    )

    print("wave_w93_render_plan_timeline_progress_smoke: ok")


if __name__ == "__main__":
    main()
