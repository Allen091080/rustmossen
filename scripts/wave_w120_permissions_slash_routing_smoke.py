#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
MODEL = ROOT / "crates/mossen-tui/src/render_model.rs"
KEYBINDING = ROOT / "crates/mossen-tui/tests/keybinding_smoke.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def reject(text: str, needle: str, label: str) -> None:
    if needle in text:
        raise SystemExit(f"forbidden {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    model = MODEL.read_text()
    keybinding = KEYBINDING.read_text()
    run_all = RUN_ALL.read_text()

    require(
        app,
        '"permissions" => {\n                if self.try_handle_permissions_mode_command(&args, args_raw)',
        "permissions conditional router",
    )
    reject(
        app,
        '"permissions" | "permission-mode" | "approval-mode" => {\n                self.open_permission_mode_picker();',
        "unconditional permissions picker interception",
    )
    require(app, "fn permission_rule_subcommand", "permission rule subcommand gate")
    require(app, "fn permission_mode_selector_subcommand", "permission mode selector gate")
    require(app, "fn handle_permission_mode_command", "direct permission mode handler")
    require(
        app,
        "block_on_current_runtime(async { directive.execute(&args, &ctx).await })",
        "directive execution works without ambient tokio reactor",
    )
    require(
        app,
        '("permissions", "Select permission mode or manage rules")',
        "slash suggestion reflects dual permissions behavior",
    )

    require(model, "fn permission_mode_match_key", "permission mode normalized matching")
    require(model, 'permission_mode_code_for_choice("full-auto")', "full-auto mode regression")
    require(model, 'permission_mode_code_for_choice("dont ask")', "dont ask mode regression")

    require(
        keybinding,
        "permissions_slash_rule_subcommands_reach_registry",
        "permissions rule routing keybinding test",
    )
    require(
        keybinding,
        "permissions_slash_accepts_direct_mode_arguments",
        "permissions direct mode keybinding test",
    )
    require(
        run_all,
        "wave_w120_permissions_slash_routing_smoke.py",
        "run_all registration",
    )

    print("wave_w120_permissions_slash_routing_smoke: ok")


if __name__ == "__main__":
    main()
