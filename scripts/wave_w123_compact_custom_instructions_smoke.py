#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
RENDER_MODEL = ROOT / "crates/mossen-tui/src/render_model.rs"
KEYBINDING = ROOT / "crates/mossen-tui/tests/keybinding_smoke.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    render_model = RENDER_MODEL.read_text()
    keybinding = KEYBINDING.read_text()
    run_all = RUN_ALL.read_text()

    require(app, "custom_instructions: Option<String>", "compact task carries custom instructions")
    require(app, "compact_engine_history_with_instructions", "custom compact entrypoint")
    require(app, "compact_instruction_tail(args, 1)", "run and plan instruction tail parsing")
    require(app, "request.custom_instructions.as_deref()", "agent compact options receive custom instructions")
    require(
        app,
        "Compacting conversation history with custom instructions...",
        "visible custom compact progress",
    )

    require(
        render_model,
        "pub custom_instructions: Option<String>",
        "compact plan render model includes custom instructions",
    )
    require(
        render_model,
        "custom instructions: {custom_instructions}",
        "compact plan renders custom instructions",
    )

    require(
        keybinding,
        "compact_slash_forwards_custom_instructions_to_compactor",
        "custom compact keybinding coverage",
    )
    require(
        keybinding,
        "/compact plan preserve permission decisions",
        "custom compact plan preview coverage",
    )

    require(
        run_all,
        "wave_w123_compact_custom_instructions_smoke.py",
        "run_all registration",
    )

    print("wave_w123_compact_custom_instructions_smoke: ok")


if __name__ == "__main__":
    main()
