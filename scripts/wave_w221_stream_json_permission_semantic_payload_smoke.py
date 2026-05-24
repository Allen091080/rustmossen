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
        '"codex_mode": permission_mode_codex_mode(mode)',
        '"terminal_control"',
        '"status_line_label": permission_mode_label(mode)',
        '"selected_index": permission_mode_option_index(mode)',
        '"aliases_accepted": true',
        "fn permission_mode_codex_mode",
        "fn permission_mode_option_index",
        '"edit_approval": edits',
        '"shell_approval": shell',
        '"legacy_internal_mode": legacy',
        "slash_command_permissions_reports_current_mode",
        "slash_command_permissions_accepts_codex_mode_aliases",
    ]:
        require(structured, needle, "permission semantic payload")

    for needle in [
        '"ask".to_string()',
        '"supervised".to_string()',
        '"read-only".to_string()',
        '"readonly".to_string()',
        '"never-ask".to_string()',
        '"permission-mode".to_string()',
        '"approval-mode".to_string()',
    ]:
        require(capabilities, needle, "permission capability aliases")

    require(
        run_all,
        "wave_w221_stream_json_permission_semantic_payload_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json permission semantic payload",
        "phase note",
    )

    print("wave_w221_stream_json_permission_semantic_payload_smoke: ok")


if __name__ == "__main__":
    main()
