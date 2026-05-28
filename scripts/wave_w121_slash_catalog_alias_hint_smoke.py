#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATE = ROOT / "crates/mossen-tui/src/state.rs"
APP = ROOT / "crates/mossen-tui/src/app.rs"
KEYBINDING = ROOT / "crates/mossen-tui/tests/keybinding_smoke.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    state = STATE.read_text()
    app = APP.read_text()
    keybinding = KEYBINDING.read_text()
    run_all = RUN_ALL.read_text()

    require(state, "pub aliases: Vec<String>", "slash catalog aliases field")
    require(state, "pub argument_hint: String", "slash catalog argument hint field")

    require(app, ".aliases()", "directive aliases collected")
    require(app, ".argument_hint()", "directive argument hints collected")
    require(app, "fn slash_command_usage_label", "help usage label helper")
    require(app, "fn slash_catalog_description", "catalog description helper")
    require(app, "fn slash_catalog_metadata", "catalog metadata helper")
    require(app, "fn slash_entry_match_score", "alias-aware typeahead scorer")
    require(app, "builtin_tui_command_aliases", "built-in alias inventory")
    require(app, "builtin_tui_command_argument_hint", "built-in argument hints")
    require(app, "aliases: {}", "alias label rendered in catalog metadata")
    require(app, 'format!("/{alias}")', "alias slash prefix formatting")

    require(
        keybinding,
        "slash_catalog_matches_aliases_and_shows_argument_hints",
        "keybinding coverage for alias and argument hint display",
    )
    require(keybinding, '"/settings"', "alias query regression")
    require(keybinding, '"/config [key=value]"', "argument hint help regression")

    require(
        run_all,
        "wave_w121_slash_catalog_alias_hint_smoke.py",
        "run_all registration",
    )

    print("wave_w121_slash_catalog_alias_hint_smoke: ok")


if __name__ == "__main__":
    main()
