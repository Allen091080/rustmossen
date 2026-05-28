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

    require(contract, "struct RenderFuzzRng", "deterministic fuzz generator")
    require(contract, "fn generated_property_fuzz_app", "generated fuzz app fixture")
    require(contract, "fn assert_scroll_state_bounded", "scroll bound invariant")
    require(
        contract,
        "app_render_contract_survives_generated_property_fuzz_matrix",
        "generated property fuzz render contract",
    )
    require(contract, "ch.is_control()", "product-wide control character assertion")
    require(contract, "property-fuzz-secret", "generated secret leak guard")
    require(contract, "property-fuzz-tail-anchor", "tail visibility anchor")
    require(
        run_all,
        "wave_w99_render_property_fuzz_matrix_smoke.py",
        "run_all registration",
    )
    require(phase, "generated property fuzz matrix", "phase record")

    print("wave_w99_render_property_fuzz_matrix_smoke: ok")


if __name__ == "__main__":
    main()
