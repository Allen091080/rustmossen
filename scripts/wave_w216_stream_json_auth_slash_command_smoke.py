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
        '"login" | "logout" => match slash_auth_response',
        "async fn slash_auth_response",
        "async fn slash_login_message",
        "fn auth_token_source_label",
        '"handoffType": "external_cli_command"',
        '"mutationSupported": false',
        '"writesAuthStateDirectly": false',
        '"tokensRedacted": true',
        '"rawEnvValuesIncluded": false',
        "slash_command_auth_returns_redacted_external_cli_handoff",
        '"command":"/login"',
        '"command":"/logout"',
    ]:
        require(structured, needle, "structured auth slash command")

    for needle in [
        "ResultKind::Auth",
        '"slash.login"',
        '"slash.logout"',
        "CommandStatus::Available",
        "SideEffect::AuthState",
        "external CLI handoff for login",
        "external CLI handoff for logout",
    ]:
        require(capabilities, needle, "auth capability")

    require(
        run_all,
        "wave_w216_stream_json_auth_slash_command_smoke",
        "run_all registration",
    )
    require(phase, "2026-05-23 Stream-json /login and /logout auth handoff", "phase note")

    print("wave_w216_stream_json_auth_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
