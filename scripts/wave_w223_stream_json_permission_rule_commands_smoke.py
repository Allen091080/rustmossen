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
        "PERMISSION_ALLOW_RULES_ENV",
        "PERMISSION_DENY_RULES_ENV",
        "fn slash_permissions_rule_response",
        '"allow" | "deny" =>',
        '"reset" | "clear" =>',
        "apply_session_permission_rule",
        "permission_rule_env_lines",
        "sync_permission_rule_env",
        '"rule_counts": permission_rule_counts()',
        '"rules": permission_rules_redacted_payload()',
        '"rule_mutation_supported": true',
        '"rule_update"',
        '"raw_patterns_included": false',
        "slash_command_permissions_rule_subcommands_update_session_env",
    ]:
        require(structured, needle, "stream-json permission rule commands")

    for needle in [
        '"list".to_string()',
        '"show".to_string()',
        '"rules".to_string()',
        '"allow".to_string()',
        '"deny".to_string()',
        '"reset".to_string()',
        '"clear".to_string()',
    ]:
        require(capabilities, needle, "permission rule capability args")

    require(
        run_all,
        "wave_w223_stream_json_permission_rule_commands_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json permission rule commands",
        "phase note",
    )

    print("wave_w223_stream_json_permission_rule_commands_smoke: ok")


if __name__ == "__main__":
    main()
