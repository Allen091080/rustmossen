#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTEXT = ROOT / "crates/mossen-agent/src/context/mod.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    context = CONTEXT.read_text()
    run_all = RUN_ALL.read_text()

    require(
        context,
        "fn build_auto_compact_boundary_message",
        "auto compact boundary builder",
    )
    require(context, "\"compact_metadata\".to_string()", "compact metadata key")
    require(context, "\"trigger\": \"auto\"", "auto trigger metadata")
    require(
        context,
        "prepend_auto_compact_boundary(",
        "auto compact result is prepended with boundary",
    )
    require(context, "result.new_messages", "auto compact result messages are rewritten")
    require(
        context,
        "ERROR_MESSAGE_NOT_ENOUGH_MESSAGES",
        "zero-compaction failure guard",
    )
    require(
        context,
        "auto compact boundary should carry metadata",
        "unit coverage for metadata boundary",
    )
    require(
        run_all,
        "wave_w124_auto_compact_boundary_metadata_smoke.py",
        "run_all registration",
    )

    print("wave_w124_auto_compact_boundary_metadata_smoke: ok")


if __name__ == "__main__":
    main()
