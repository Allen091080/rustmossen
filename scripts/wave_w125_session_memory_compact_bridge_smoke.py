#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTEXT = ROOT / "crates/mossen-agent/src/context/mod.rs"
SM_COMPACT = ROOT / "crates/mossen-agent/src/services/compact/session_memory_compact.rs"
SM_UTILS = ROOT / "crates/mossen-agent/src/services/session_memory/utils.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    context = CONTEXT.read_text()
    sm_compact = SM_COMPACT.read_text()
    sm_utils = SM_UTILS.read_text()
    run_all = RUN_ALL.read_text()

    require(
        context,
        "try_session_memory_compaction(",
        "auto compact invokes session memory compaction",
    )
    require(
        context,
        "build_post_compact_messages(&result)",
        "session memory compact result enters model context",
    )
    require(
        sm_compact,
        "build_session_memory_compaction_result",
        "non-empty session memory compact builder",
    )
    require(
        sm_compact,
        "\"session_memory\".to_string()",
        "session memory compact metadata trigger",
    )
    require(
        sm_compact,
        "get_compact_user_summary_message(&summary, true, None, Some(true))",
        "session memory summary is wrapped as compact continuation context",
    )
    require(
        sm_compact,
        "session_memory_compaction_uses_memory_and_preserves_recent_messages",
        "session memory compact unit coverage",
    )
    require(
        sm_utils,
        "MOSSEN_SESSION_MEMORY_PATH",
        "explicit session memory path reader",
    )
    require(
        sm_utils,
        "session_memory.json",
        "json session memory reader fallback",
    )
    require(
        run_all,
        "wave_w125_session_memory_compact_bridge_smoke.py",
        "run_all registration",
    )

    print("wave_w125_session_memory_compact_bridge_smoke: ok")


if __name__ == "__main__":
    main()
