#!/usr/bin/env python3
"""W234 - slash result event patches carry idempotency and sequence guards."""

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
        "terminal_attach_event_sequence_to_region_payloads",
        "terminal_slash_result_region_hash",
        '"regionHash"',
        '"idempotencyKey"',
        '"skipIfRegionHashUnchanged"',
        '"eventSequenceGuard"',
        '"dropWhenSuperseded"',
        '"skipIfRegionAbsent"',
        '"sourceEventSequence"',
    ]:
        require(bridge, token, "slash result patch idempotency", failures)

    for token in [
        "slash_result_event_patch_idempotency_guards",
        "terminal_slash_result_event_patch_idempotency_guards",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w234_stream_json_slash_result_patch_idempotency_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json slash result patch idempotency guards",
        "phase note",
        failures,
    )

    if failures:
        print("=== W234 stream-json slash result patch idempotency smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w234_stream_json_slash_result_patch_idempotency_smoke: ok")


if __name__ == "__main__":
    main()
