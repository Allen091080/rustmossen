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
        "compact_plan_render_model(",
        "compact plan semantic render-model call",
    )
    for forbidden in [
        "fn compact_plan_summary_preview",
        "fn compact_plan_blocks_inline",
        "fn compact_plan_tool_result_inline",
        "fn compact_plan_inline",
        "fn compact_plan_role_label",
        "tool.input.to_string()",
    ]:
        reject(app, forbidden, "root compact plan formatter")

    require(
        model,
        "pub fn compact_plan_summary_preview_from_messages",
        "semantic compact plan preview helper",
    )
    require(
        model,
        "tool_input_summary_from_value(&tool.input)",
        "semantic tool input summary inside compact plan preview",
    )
    require(
        model,
        "compact_plan_summary_preview_from_messages(messages)",
        "semantic compact plan preview token estimate call",
    )
    require(
        model,
        "compact_plan_preview_formats_messages_without_raw_tool_json",
        "compact plan preview regression",
    )
    require(
        run_all,
        "wave_w111_render_compact_plan_preview_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w111_render_compact_plan_preview_boundary_smoke: ok")


if __name__ == "__main__":
    main()
