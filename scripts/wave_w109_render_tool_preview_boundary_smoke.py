#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
MODEL = ROOT / "crates/mossen-tui/src/render_model.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def reject(text: str, needle: str, label: str) -> None:
    if needle in text:
        raise SystemExit(f"forbidden {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    model = MODEL.read_text()
    run_all = RUN_ALL.read_text()

    require(
        app,
        "tool_call_preview_from_input(&prompt.tool_name, &confirm.raw_input)",
        "approval preview semantic helper call",
    )
    require(
        app,
        "tool_call_preview_from_input(&name, &input)",
        "tool-use transcript preview semantic helper call",
    )
    for forbidden in [
        "fn format_tool_call_preview",
        "fn format_tool_input",
        "fn preview_block",
        "fn simple_unified_diff",
    ]:
        reject(app, forbidden, "root tool preview formatter")

    require(
        model,
        "pub fn tool_call_preview_from_input",
        "semantic tool-call preview helper",
    )
    require(
        model,
        "pub fn tool_input_summary_from_value",
        "semantic tool-input summary helper",
    )
    require(
        model,
        "tool_call_preview_formats_known_tool_inputs_without_root_helpers",
        "semantic tool preview regression",
    )
    require(
        run_all,
        "wave_w109_render_tool_preview_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w109_render_tool_preview_boundary_smoke: ok")


if __name__ == "__main__":
    main()
