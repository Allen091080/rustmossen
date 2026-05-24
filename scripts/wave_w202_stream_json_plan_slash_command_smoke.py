#!/usr/bin/env python3
"""W202 - stream-json /plan slash command enters and exits plan mode."""

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
        '"plan" => match slash_plan_response(&args)',
        "fn slash_plan_response",
        "fn slash_plan_summary_response",
        "slash_command_plan_enters_and_exits_plan_mode",
        '"plan"',
        '"plan-mode"',
    ]:
        require(structured, token, "plan slash handler", failures)

    for token in [
        "slash.plan",
        "ResultKind::Plan",
        '"plan-mode"',
        "SwitchesPermissionMode",
        "crates/mossen-cli/src/structured_io.rs:slash_command/plan",
    ]:
        require(caps, token, "plan slash capability", failures)

    require(
        run_all,
        "wave_w202_stream_json_plan_slash_command_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json /plan slash command bridge",
        "phase note",
        failures,
    )

    if failures:
        print("=== W202 stream-json /plan slash command smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w202_stream_json_plan_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
