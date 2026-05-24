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
    app_runtime = app.split("#[cfg(test)]", 1)[0]
    model = MODEL.read_text()
    run_all = RUN_ALL.read_text()

    require(
        app_runtime,
        "compact_plan_body_from_model(&self.compact_plan_model())",
        "app compact plan render-model body call",
    )
    require(
        app_runtime,
        "compact_status_body_from_model(&self.compact_status_model())",
        "app compact status render-model body call",
    )
    require(app_runtime, "fn compact_plan_model(&self)", "app compact plan model adapter")
    require(app_runtime, "fn compact_status_model(&self)", "app compact status model adapter")

    for forbidden in [
        "struct CompactPlanEstimate",
        "fn compact_plan_estimate",
        "fn compact_plan_summary_tokens",
        "fn compact_plan_body(&self)",
        "fn compact_status_body(&self)",
        '"Compact plan\\nstate:',
        '"Compact status\\nstate:',
    ]:
        reject(app_runtime, forbidden, "root compact modal formatter")

    for needle, label in [
        ("pub struct CompactPlanRenderModel", "compact plan render model"),
        ("pub fn compact_plan_render_model", "compact plan model builder"),
        ("pub fn compact_plan_body_from_model", "compact plan body formatter"),
        ("pub struct CompactStatusRenderModel", "compact status render model"),
        ("pub fn compact_status_body_from_model", "compact status body formatter"),
        ("mossen_agent::token_estimation::estimate_messages_tokens", "semantic token estimate"),
        ("compact_plan_model_formats_dry_run_body", "compact plan regression"),
        ("compact_status_model_formats_lifecycle_body", "compact status regression"),
    ]:
        require(model, needle, label)

    require(
        run_all,
        "wave_w119_render_compact_modal_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w119_render_compact_modal_boundary_smoke: ok")


if __name__ == "__main__":
    main()
