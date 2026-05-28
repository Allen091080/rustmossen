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
        '"approval_submitted"',
        '"approval submitted: {label}"',
        "terminal_approval_bridge_status_retires_blocking_region_after_submit",
        'frame["status"]["blocking"], false',
        'frame["terminal"]["approval"]["blocking"], false',
        'region["id"] == "approval"',
        'frame["changes"]["retiredRegions"]',
    ):
        require(render_events, token, f"submitted approval render token {token}")

    for token in (
        '"terminal_approval_submitted_nonblocking"',
        '"terminal_approval_submitted_retires_blocking_region"',
    ):
        require(structured_io, token, f"status submitted approval metadata {token}")

    require(
        run_all,
        "wave_w167_stream_json_terminal_approval_submitted_region_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal approval submitted region retirement",
        "phase note",
    )

    print("wave_w167_stream_json_terminal_approval_submitted_region_smoke: ok")


if __name__ == "__main__":
    main()
