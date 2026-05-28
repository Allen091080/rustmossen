#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
CAPABILITIES = ROOT / "crates/mossen-agent/src/services/root/slash_command_capabilities.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    structured_io = STRUCTURED_IO.read_text()
    capabilities = CAPABILITIES.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    require(
        structured_io,
        "handle_slash_command_control_request",
        "slash_command control_request handler",
    )
    require(
        structured_io,
        'Some("slash_command")',
        "slash_command subtype interception",
    )
    require(
        structured_io,
        "slash_command_result",
        "slash_command structured result subtype",
    )
    require(
        structured_io,
        "parse_slash_command_request",
        "slash command parser",
    )
    for token in (
        "slash_help_response",
        "slash_capabilities_response",
        "slash_status_response",
        "slash_permissions_response",
        "build_compact_slash_response",
    ):
        require(structured_io, token, f"runtime slash branch {token}")
    require(
        structured_io,
        "enqueue_pending_compact_request",
        "compact slash bridge enqueues through compact control buffer",
    )
    require(
        structured_io,
        "CompactMode::Manual",
        "compact slash bridge uses manual compact mode",
    )
    require(
        structured_io,
        "unwired_slash_command",
        "unwired known commands fail explicitly",
    )
    for test_name in (
        "slash_command_help_control_request_responds_with_manifest_summary",
        "slash_command_permissions_reports_current_mode",
        "slash_command_compact_preview_enqueues_dry_run_request",
        "slash_command_unknown_returns_error_response",
    ):
        require(structured_io, test_name, f"unit test {test_name}")

    for token in (
        "ArgsMode::Subcommand",
        "ResultKind::Compact",
        '"slash.compact"',
        '"plan".to_string()',
        '"--confirm".to_string()',
    ):
        require(capabilities, token, f"compact capability token {token}")

    require(
        run_all,
        "wave_w130_stream_json_slash_runtime_bridge_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json slash runtime bridge",
        "phase note",
    )

    print("wave_w130_stream_json_slash_runtime_bridge_smoke: ok")


if __name__ == "__main__":
    main()
