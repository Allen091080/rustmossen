#!/usr/bin/env python3
"""W205 - stream-json /mcp slash command is wired as a safe read-only snapshot."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    caps = (
        ROOT / "crates/mossen-agent/src/services/root/slash_command_capabilities.rs"
    ).read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        '"mcp" => match slash_mcp_response(&args).await',
        "async fn slash_mcp_response",
        "fn mcp_connection_state_label",
        "mcp_runtime_status::snapshot().await",
        "slash_command_mcp_inventory_returns_redacted_snapshot",
    ]:
        require(structured, token, "mcp slash handler", failures)

    for token in [
        '"rawConfigRedacted"',
        '"toolSchemasRedacted"',
        '"instructionsRedacted"',
        '"errorDetailsRedacted"',
        '"mutationSupported"',
    ]:
        require(structured, token, "mcp redaction payload", failures)

    require(
        caps,
        "structured_io.rs:slash_command/mcp",
        "capability source",
        failures,
    )
    require(
        run_all,
        "wave_w205_stream_json_mcp_slash_command_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json /mcp slash command bridge",
        "phase note",
        failures,
    )

    if failures:
        print("=== W205 stream-json /mcp slash command smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w205_stream_json_mcp_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
