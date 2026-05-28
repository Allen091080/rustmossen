#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "crates/mossen-tui/tests/render_contract.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    contract = CONTRACT.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    require(contract, "fn streaming_soak_app", "streaming soak fixture")
    require(contract, "fn push_streaming_soak_delta", "streaming soak delta helper")
    require(
        contract,
        "app_render_contract_simulated_streaming_soak_keeps_scroll_and_budget",
        "simulated streaming soak render contract",
    )
    require(contract, "streaming-soak-head-anchor", "manual scroll anchor")
    require(contract, "streaming-soak-tail-anchor", "restored tail anchor")
    require(contract, "Duration::from_millis(1_500)", "streaming soak budget guard")
    require(contract, "assert_scroll_state_bounded", "scroll bounds invariant")
    require(
        run_all,
        "wave_w100_render_streaming_soak_budget_smoke.py",
        "run_all registration",
    )
    require(phase, "simulated streaming soak budget", "phase record")

    print("wave_w100_render_streaming_soak_budget_smoke: ok")


if __name__ == "__main__":
    main()
