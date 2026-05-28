#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNTIME_STATUS = ROOT / "crates/mossen-agent/src/services/root/runtime_status.rs"
ROOT_MOD = ROOT / "crates/mossen-agent/src/services/root/mod.rs"
DIALOGUE = ROOT / "crates/mossen-agent/src/dialogue.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    runtime_status = RUNTIME_STATUS.read_text()
    root_mod = ROOT_MOD.read_text()
    dialogue = DIALOGUE.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "AgentRuntimeStatusSnapshot",
        "record_agent_dialogue_start",
        "record_agent_dialogue_finish",
        "snapshot_agent_runtime_status",
        "runtime_status_tracks_start_and_finish",
        "active_dialogues",
        "total_dialogues_started",
    ):
        require(runtime_status, token, f"runtime status token {token}")

    require(root_mod, "pub mod runtime_status;", "root module export")

    for token in (
        "record_agent_dialogue_start(&session_id, &spec.model)",
        "record_agent_dialogue_finish(terminal_reason.as_deref(), error.as_deref())",
    ):
        require(dialogue, token, f"dialogue runtime status token {token}")

    for token in (
        "snapshot_agent_runtime_status",
        "pending_compact_status",
        '"agent": snapshot_agent_runtime_status()',
        '"compact": compact',
        '"queues"',
        "slash_command_status_reports_runtime_snapshot",
    ):
        require(structured_io, token, f"StructuredIO status token {token}")

    require(
        run_all,
        "wave_w132_stream_json_runtime_status_snapshot_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json runtime status snapshot",
        "phase note",
    )

    print("wave_w132_stream_json_runtime_status_snapshot_smoke: ok")


if __name__ == "__main__":
    main()
