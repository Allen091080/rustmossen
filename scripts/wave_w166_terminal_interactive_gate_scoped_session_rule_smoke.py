#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TYPES = ROOT / "crates/mossen-agent/src/types.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    types = TYPES.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "interactive_gate_session_rule_key",
        "interactive_gate_shell_command_rule",
        "interactive_gate_rule_text",
        'matches!(tool_name, "Bash" | "PowerShell" | "Execute")',
        'format!("command:{clean_tool_name}:{command}")',
        'format!("tool:{clean_tool_name}")',
        "interactive_gate_allow_always_is_scoped_to_exact_shell_command",
        "interactive_gate_allow_always_falls_back_to_tool_scope_without_shell_command",
    ):
        require(types, token, f"interactive gate scoped session token {token}")

    for token in (
        '"terminal_interactive_gate_scoped_allow_always"',
        '"terminal_interactive_gate_exact_command_rule"',
    ):
        require(structured_io, token, f"status scoped gate metadata {token}")

    require(
        run_all,
        "wave_w166_terminal_interactive_gate_scoped_session_rule_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Terminal interactive gate scoped session rule",
        "phase note",
    )

    print("wave_w166_terminal_interactive_gate_scoped_session_rule_smoke: ok")


if __name__ == "__main__":
    main()
