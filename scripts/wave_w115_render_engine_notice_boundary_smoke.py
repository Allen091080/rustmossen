#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
LIFECYCLE = ROOT / "crates/mossen-tui/src/render_lifecycle.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def reject(text: str, needle: str, label: str) -> None:
    if needle in text:
        raise SystemExit(f"forbidden {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    lifecycle = LIFECYCLE.read_text()
    run_all = RUN_ALL.read_text()

    require(
        app,
        "compact_boundary_transcript_facts(",
        "root compact-boundary semantic facts call",
    )
    require(
        app,
        "api_retry_transcript_message(",
        "root api-retry semantic message call",
    )
    for forbidden in [
        '"(compact) tokens {} -> {}"',
        '"Tokens {} -> {}"',
        '"API retry {}/{} in {}ms: {}"',
    ]:
        reject(app, forbidden, "root engine-notice transcript formatting")

    require(
        lifecycle,
        "pub struct CompactBoundaryTranscriptFacts",
        "compact-boundary transcript facts model",
    )
    require(
        lifecycle,
        "pub fn compact_boundary_transcript_facts",
        "compact-boundary transcript facts helper",
    )
    require(
        lifecycle,
        "pub fn api_retry_transcript_message",
        "api-retry transcript message helper",
    )
    require(
        lifecycle,
        "engine_notice_transcript_facts_format_compact_and_retry_rows",
        "engine-notice transcript facts regression",
    )
    require(
        run_all,
        "wave_w115_render_engine_notice_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w115_render_engine_notice_boundary_smoke: ok")


if __name__ == "__main__":
    main()
