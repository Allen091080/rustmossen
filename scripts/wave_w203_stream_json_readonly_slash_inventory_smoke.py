#!/usr/bin/env python3
"""W203 - stream-json read-only slash inventory commands are wired."""

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
        "fn slash_readonly_runtime_inventory_response",
        "fn slash_cost_response",
        "fn slash_hooks_response",
        "fn slash_memory_response",
        "fn slash_skills_response",
        "fn slash_plugin_response",
        "fn slash_agents_response",
        "slash_command_readonly_inventory_commands_return_safe_snapshots",
    ]:
        require(structured, token, "readonly slash handlers", failures)

    for command in ['"cost"', '"hooks"', '"memory"', '"skills"', '"plugin"', '"agents"']:
        require(structured, command, "wired command list", failures)

    for token in [
        "structured_io.rs:slash_command/cost",
        "structured_io.rs:slash_command/hooks",
        "structured_io.rs:slash_command/memory",
        "structured_io.rs:slash_command/skills",
        "structured_io.rs:slash_command/plugin",
        "structured_io.rs:slash_command/agents",
    ]:
        require(caps, token, "capability source", failures)

    require(
        run_all,
        "wave_w203_stream_json_readonly_slash_inventory_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json read-only slash inventory commands",
        "phase note",
        failures,
    )

    if failures:
        print("=== W203 stream-json read-only slash inventory smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w203_stream_json_readonly_slash_inventory_smoke: ok")


if __name__ == "__main__":
    main()
