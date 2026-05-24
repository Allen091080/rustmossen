#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACCESS = ROOT / "crates/mossen-commands/src/access.rs"
APP = ROOT / "crates/mossen-tui/src/app.rs"
KEYBINDING = ROOT / "crates/mossen-tui/tests/keybinding_smoke.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    access = ACCESS.read_text()
    app = APP.read_text()
    keybinding = KEYBINDING.read_text()
    run_all = RUN_ALL.read_text()

    require(
        access,
        "pub const PERMISSION_ALLOW_RULES_ENV",
        "allow rule env constant",
    )
    require(access, "pub const PERMISSION_DENY_RULES_ENV", "deny rule env constant")
    require(access, "permission_rules_text(ctx)", "permissions command reads rules")
    require(access, "permissions_list_reads_session_rule_env", "command unit coverage")

    require(app, "struct SessionPermissionRules", "session rule state")
    require(app, "struct SessionPermissionGate", "session permission gate")
    require(app, "permission_rules_from_env", "rule env hydration")
    require(
        app,
        "apply_permission_rule_command_side_effect",
        "slash command rule side effect",
    )
    require(app, "sync_permission_rule_env", "rule env sync")
    require(app, "permission_rules_match", "rule matcher")
    require(app, "SessionPermissionGate::new", "gate wrapping at prompt submit")
    require(
        app,
        "session_permission_gate_applies_rules_before_fallback",
        "gate unit coverage",
    )

    require(
        keybinding,
        "MOSSEN_PERMISSION_ALLOW_RULES",
        "allow rule keybinding assertion",
    )
    require(
        keybinding,
        "MOSSEN_PERMISSION_DENY_RULES",
        "deny rule keybinding assertion",
    )

    require(
        run_all,
        "wave_w122_permission_rule_gate_smoke.py",
        "run_all registration",
    )

    print("wave_w122_permission_rule_gate_smoke: ok")


if __name__ == "__main__":
    main()
