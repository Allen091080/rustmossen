#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
MODEL = ROOT / "crates/mossen-tui/src/render_model.rs"
CONTRACT = ROOT / "crates/mossen-tui/tests/render_contract.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    model = MODEL.read_text()
    contract = CONTRACT.read_text()
    run_all = RUN_ALL.read_text()

    require(model, "pub fn focused() -> Self", "focused statusline preset")
    require(model, "FooterPreset::Focused", "focused preset enum branch")
    require(model, "pub fn matching_preset", "preset shape detector")
    require(model, '"all built-in status facts plus external status"', "full preset description")
    require(app, "M/C/D/F apply presets", "statusline modal preset hint")
    require(app, '\"focused\" | \"focus\" | \"codex\"', "codex focused preset alias")
    require(app, "footer_statusline_presets_have_distinct_render_shapes", "preset unit test")
    require(app, "footer_statusline_codex_alias_applies_focused_preset", "codex alias unit test")
    require(
        contract,
        "app_render_contract_statusline_presets_are_visible_and_codex_focused",
        "product statusline preset contract",
    )
    require(contract, "C focused", "focused preset modal assertion")
    require(contract, "reasoning:high", "focused footer reasoning assertion")
    require(
        run_all,
        "wave_w97_render_statusline_presets_smoke.py",
        "run_all registration",
    )

    print("wave_w97_render_statusline_presets_smoke: ok")


if __name__ == "__main__":
    main()
