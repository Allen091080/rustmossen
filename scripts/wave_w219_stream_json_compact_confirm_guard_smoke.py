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
        'unwrap_or("preview")',
        '"run_requires_confirm": true',
        '"requires_confirm": true',
        'next": "use /compact run --confirm to queue conversation compaction"',
        'args.iter().skip(1).any(|arg| arg == "--confirm")',
        "fn compact_custom_instructions",
        'filter(|arg| arg.as_str() != "--confirm")',
        "slash_command_compact_run_requires_confirm",
        "slash_command_compact_run_confirm_enqueues_real_request",
    ]:
        require(structured, needle, "stream-json compact confirm guard")

    for needle in [
        '"slash.compact"',
        "requires_confirmation",
        '"--confirm".to_string()',
        '"confirm".to_string()',
        "ResultKind::Compact",
    ]:
        require(capabilities, needle, "compact capability confirm metadata")

    require(
        run_all,
        "wave_w219_stream_json_compact_confirm_guard_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json compact confirmation guard",
        "phase note",
    )

    print("wave_w219_stream_json_compact_confirm_guard_smoke: ok")


if __name__ == "__main__":
    main()
