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
        "let input_summary = tool_input_summary_from_value(&input);",
        "permission summary semantic helper call",
    )
    reject(
        app,
        "serde_json::to_string(&input).unwrap_or_else(|_| \"<unserialisable input>\".to_string())",
        "root raw JSON permission summary",
    )
    require(
        app,
        'assert_eq!(confirm.input_summary, "ls -la");',
        "approval summary regression",
    )
    require(
        model,
        "pub fn tool_input_summary_from_value",
        "semantic input summary helper",
    )
    require(
        run_all,
        "wave_w110_render_permission_summary_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w110_render_permission_summary_boundary_smoke: ok")


if __name__ == "__main__":
    main()
