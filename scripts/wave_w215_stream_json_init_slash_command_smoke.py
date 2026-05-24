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
        "async fn slash_init_response",
        "async fn slash_init_prompt",
        '"command": "init"',
        '"handoffType": "agent_prompt"',
        '"modelTurnRequired": true',
        '"writesFilesDirectly": false',
        '"usesNormalToolPermissions": true',
        '"requiresToolApprovalForWrites": true',
        "mossen_commands::init::InitDirective",
        "slash_command_init_returns_agent_prompt_handoff_without_direct_write",
        '"command":"/init"',
    ]:
        require(structured, needle, "structured init slash command")

    for needle in [
        "ResultKind::Init",
        '"slash.init"',
        "CommandStatus::Available",
        "SideEffect::WritesFiles",
        '"run".to_string()',
        "prompt handoff for agent-driven MOSSEN.md initialization",
    ]:
        require(capabilities, needle, "init capability")

    require(
        run_all,
        "wave_w215_stream_json_init_slash_command_smoke",
        "run_all registration",
    )
    require(phase, "2026-05-23 Stream-json /init slash command bridge", "phase note")

    print("wave_w215_stream_json_init_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
