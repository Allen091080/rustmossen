#!/usr/bin/env python3
"""W207 - stream-json /permissions exposes Codex-style mode choices."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    caps = (
        ROOT / "crates/mossen-agent/src/services/root/slash_command_capabilities.rs"
    ).read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "fn permission_mode_options",
        "fn permission_mode_option_value",
        '"mode_options"',
        '"codex_approval_modes"',
        '"selected_option"',
        '"suggest"',
        '"auto-edit"',
        '"full-auto"',
        "slash_command_permissions_accepts_codex_mode_aliases",
    ]:
        require(structured, token, "permission mode choice payload", failures)

    for token in [
        '"suggest".to_string()',
        '"auto-edit".to_string()',
        '"full-auto".to_string()',
        '"choose".to_string()',
    ]:
        require(caps, token, "permission capability args", failures)

    require(
        run_all,
        "wave_w207_stream_json_permission_mode_choices_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json permission mode choices",
        "phase note",
        failures,
    )

    if failures:
        print("=== W207 stream-json permission mode choices smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w207_stream_json_permission_mode_choices_smoke: ok")


if __name__ == "__main__":
    main()
