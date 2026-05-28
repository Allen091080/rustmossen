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
        "fn permission_mode_picker_payload",
        '"kind": "permission_mode_picker"',
        '"layout": "segmented_control"',
        '"codex_value": codex_value',
        '"codex_order": ["suggest", "plan", "auto-edit", "full-auto", "dont-ask"]',
        '"selected_value": permission_mode_option_value(mode)',
        'Some("mode" | "set" | "choose" | "select")',
        "slash_command_permissions_reports_current_mode",
    ]:
        require(structured, needle, "permission mode picker payload")

    for needle in [
        "fn compact_preview_payload",
        "fn compact_action_options",
        '"kind": "compact_preview"',
        '"safe_point": "dialogue_safe_point"',
        '"expected_status_event": "compact_request_status"',
        '"confirm_command": "/compact run --confirm"',
        '"will_mutate_history": !dry_run',
        '"dry-run" | "dryrun"',
        '"action": action',
        '"requested_action": requested_action',
        '"command": "/compact preview"',
        "slash_command_compact_preview_enqueues_dry_run_request",
        "slash_command_compact_run_confirm_enqueues_real_request",
    ]:
        require(structured, needle, "compact preview control payload")

    for needle in [
        '"select".to_string()',
        '"picker".to_string()',
        '"dry-run".to_string()',
        '"dryrun".to_string()',
    ]:
        require(capabilities, needle, "slash capability accepted args")

    require(
        run_all,
        "wave_w323_stream_json_slash_compact_permissions_controls_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-24 Stream-json slash compact and permission controls",
        "phase note",
    )

    print("wave_w323_stream_json_slash_compact_permissions_controls_smoke: ok")


if __name__ == "__main__":
    main()
