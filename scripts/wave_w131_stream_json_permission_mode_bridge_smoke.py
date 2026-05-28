#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
REPL = ROOT / "crates/mossen-cli/src/repl.rs"
DIALOGUE = ROOT / "crates/mossen-agent/src/dialogue.rs"
CAPABILITIES = ROOT / "crates/mossen-agent/src/services/root/slash_command_capabilities.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    structured_io = STRUCTURED_IO.read_text()
    repl = REPL.read_text()
    dialogue = DIALOGUE.read_text()
    capabilities = CAPABILITIES.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "const PERMISSION_MODE_ENV",
        "parse_permission_mode_arg",
        "slash_permissions_summary_response",
        "std::env::set_var(PERMISSION_MODE_ENV, mode.as_str())",
        "slash_command_permissions_mode_updates_session_env",
        '"/permission-mode"',
        '"fullauto"',
    ):
        require(structured_io, token, f"StructuredIO permission mode token {token}")

    require(repl, "fn session_permission_mode_from_env", "oneshot permission env reader")
    if repl.count("permission_mode: session_permission_mode_from_env()") < 2:
        raise SystemExit("missing PromptParams/OrchestratorConfig permission env wiring")

    for token in (
        "effective_permission_mode(spec.permission_mode)",
        "fn effective_permission_mode",
        "effective_permission_mode_prefers_session_env_override",
    ):
        require(dialogue, token, f"dialogue permission mode token {token}")

    for token in (
        "SideEffect::SwitchesPermissionMode",
        "ArgsMode::Subcommand",
        '"permission-mode".to_string()',
        '"bypassPermissions".to_string()',
    ):
        require(capabilities, token, f"capability permission token {token}")

    require(
        run_all,
        "wave_w131_stream_json_permission_mode_bridge_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json permission mode bridge",
        "phase note",
    )

    print("wave_w131_stream_json_permission_mode_bridge_smoke: ok")


if __name__ == "__main__":
    main()
