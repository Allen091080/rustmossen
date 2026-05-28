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
        "async fn slash_ide_response",
        '"command": "ide"',
        '"externalScanRun": false',
        '"processScanRun": false',
        '"openCommandRun": false',
        '"mcpRuntimeSnapshot": true',
        '"supportedTransports": ["sse-ide", "ws-ide"]',
        '"pendingLspDiagnosticCount"',
        "get_pending_lsp_diagnostic_count()",
        "slash_command_ide_returns_readonly_mcp_ide_snapshot",
        '"command":"/ide"',
    ]:
        require(structured, needle, "structured ide slash command")

    for needle in [
        "ResultKind::Ide",
        '"slash.ide"',
        "CommandStatus::Available",
        '"editor".to_string()',
        '"diagnostics".to_string()',
        "redacted read-only IDE/MCP connection snapshot",
    ]:
        require(capabilities, needle, "ide capability")

    require(
        run_all,
        "wave_w213_stream_json_ide_slash_command_smoke",
        "run_all registration",
    )
    require(phase, "2026-05-23 Stream-json /ide slash command bridge", "phase note")

    print("wave_w213_stream_json_ide_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
