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
        "items: permission_mode_choices()",
        "permission picker semantic choices",
    )
    require(
        app,
        "permission_mode_code_for_raw(",
        "permission request code normalization call",
    )
    for forbidden in [
        "const PERMISSION_MODE_CHOICES",
        "fn permission_mode_display_label",
        "fn permission_mode_choice_index",
        "fn permission_mode_code_for_raw",
        "fn permission_mode_code_for_choice",
    ]:
        reject(app, forbidden, "root permission mode choice model")

    require(
        model,
        "pub struct PermissionModeChoiceRenderModel",
        "permission mode choice model",
    )
    require(
        model,
        "pub fn permission_mode_choices",
        "permission mode choices helper",
    )
    require(
        model,
        "permission_mode_choices_normalize_labels_and_codes",
        "permission mode normalization regression",
    )
    require(
        run_all,
        "wave_w113_render_permission_mode_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w113_render_permission_mode_boundary_smoke: ok")


if __name__ == "__main__":
    main()
