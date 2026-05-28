#!/usr/bin/env python3
"""W230 - slash result render events carry bounded redacted previews."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    bridge = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "terminal_enrich_slash_result_event_value",
        "terminal_copy_slash_result_preview_payload",
        "terminal_copy_slash_result_preview_fields",
        "slash_command_result_event_payload_carries_bounded_redacted_preview",
        "slash_command_result_event_only_reducer_renders_preview_region",
        '"previewLines"',
        '"rawResponseIncluded"',
        '"redacted"',
        "secret-token-value",
    ]:
        require(bridge, token, "slash result event preview", failures)

    for token in [
        "slash_result_event_preview_payload",
        "terminal_slash_result_event_preview_payload",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w230_stream_json_slash_result_event_preview_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json slash result event preview payload",
        "phase note",
        failures,
    )

    if failures:
        print("=== W230 stream-json slash result event preview smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w230_stream_json_slash_result_event_preview_smoke: ok")


if __name__ == "__main__":
    main()
