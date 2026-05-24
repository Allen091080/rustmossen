#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    agent_types = read("crates/mossen-agent/src/types.rs")
    dialogue = read("crates/mossen-agent/src/dialogue.rs")
    structured = read("crates/mossen-cli/src/structured_io.rs")
    capabilities = read("crates/mossen-agent/src/services/root/slash_command_capabilities.rs")
    run_all = read("scripts/run_all_smoke.sh")
    phase = read("phases/03g-rendering-product-grade-plan.md")

    for needle in [
        '"plan" | "readonly" | "read" => Self::Plan',
        '"default" | "supervised" | "suggest" | "ask" | "" => Self::Default',
        '"dontask" | "dontprompt" | "neverask" => Self::DontAsk',
    ]:
        require(agent_types, needle, "agent permission mode alias parse")

    for needle in [
        'PermissionMode::parse("suggest")',
        'PermissionMode::parse("ask")',
        'PermissionMode::parse("read-only")',
        'PermissionMode::parse("readonly")',
        'PermissionMode::parse("never-ask")',
        "effective_permission_mode_prefers_session_env_override",
        "permission_mode_decision(",
    ]:
        require(dialogue, needle, "agent permission execution coverage")

    require(
        structured,
        '"codex_mode": permission_mode_codex_mode(mode)',
        "stream-json permission semantic payload",
    )

    for needle in [
        '"read-only".to_string()',
        '"readonly".to_string()',
        '"suggest".to_string()',
        '"ask".to_string()',
        '"never-ask".to_string()',
    ]:
        require(capabilities, needle, "stream-json permission alias capability")

    require(
        run_all,
        "wave_w222_agent_permission_alias_parity_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Agent permission alias parity",
        "phase note",
    )

    print("wave_w222_agent_permission_alias_parity_smoke: ok")


if __name__ == "__main__":
    main()
