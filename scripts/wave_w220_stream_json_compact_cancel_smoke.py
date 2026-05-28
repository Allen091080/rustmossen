#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    structured = read("crates/mossen-cli/src/structured_io.rs")
    capabilities = read("crates/mossen-agent/src/services/root/slash_command_capabilities.rs")
    run_all = read("scripts/run_all_smoke.sh")
    phase = read("phases/03g-rendering-product-grade-plan.md")

    for needle in [
        "clear_pending_compact_request",
        '"cancel" | "stop" =>',
        '"action": "cancel"',
        '"cancelled": pending.is_some()',
        '"pending": false',
        '"had_custom_instructions": pending',
        "slash_command_compact_cancel_clears_pending_request",
        '"request_id":"slash-compact-cancel"',
    ]:
        require(structured, needle, "stream-json compact cancel path")

    for needle in [
        '"slash.compact"',
        '"cancel".to_string()',
        '"stop".to_string()',
        '"run".to_string()',
        '"--confirm".to_string()',
    ]:
        require(capabilities, needle, "compact capability cancel metadata")

    require(
        run_all,
        "wave_w220_stream_json_compact_cancel_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json compact cancellation control",
        "phase note",
    )

    print("wave_w220_stream_json_compact_cancel_smoke: ok")


if __name__ == "__main__":
    main()
