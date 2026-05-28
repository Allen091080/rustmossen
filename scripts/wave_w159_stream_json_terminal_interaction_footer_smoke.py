#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDER_EVENTS = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    render_events = RENDER_EVENTS.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "terminal_interaction_value",
        "terminal_interaction_hints",
        '"visibleInFooter"',
        '"manualScrollKeys"',
        '"commandToggleKey"',
        '"diffToggleKey"',
        "keys: {}",
        "o expand cmd",
        "d expand diff",
        "PgUp hold",
        "terminal_footer_exposes_contextual_keymap_controls",
        "actions: {}",
    ):
        require(render_events, token, f"interaction footer token {token}")

    for token in (
        '"terminal_footer_keymap_hints"',
        '"terminal_contextual_interaction_metadata"',
        '"terminal_widget_key_hints"',
        '"terminal_approval_decision_hints"',
    ):
        require(structured_io, token, f"status interaction metadata {token}")

    require(
        run_all,
        "wave_w159_stream_json_terminal_interaction_footer_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal interaction footer hints",
        "phase note",
    )

    print("wave_w159_stream_json_terminal_interaction_footer_smoke: ok")


if __name__ == "__main__":
    main()
