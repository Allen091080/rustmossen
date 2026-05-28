#!/usr/bin/env python3
"""W42 — current Rust stream-json slash capability wrappers smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W42",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="slash_help_returns_manifest_summary",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "structured_io::tests::slash_command_help_control_request_responds_with_manifest_summary",
                ),
            ),
            Step(
                name="readonly_inventory_commands_return_safe_snapshots",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "structured_io::tests::slash_command_readonly_inventory_commands_return_safe_snapshots",
                ),
            ),
            Step(
                name="slash_mcp_returns_redacted_runtime_snapshot",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "structured_io::tests::slash_command_mcp_inventory_returns_redacted_snapshot",
                ),
            ),
            Step(
                name="slash_ide_returns_readonly_mcp_ide_snapshot",
                command=cargo_test(
                    "-p",
                    "mossen-cli",
                    "structured_io::tests::slash_command_ide_returns_readonly_mcp_ide_snapshot",
                ),
            ),
        ],
        checks=[
            source_check(
                "capability_manifest_contains_readonly_wrappers",
                "crates/mossen-agent/src/services/root/slash_command_capabilities.rs",
                [
                    '"slash.skills"',
                    '"slash.mcp"',
                    '"slash.plugin"',
                    '"slash.agents"',
                    "SideEffect::None",
                    "ResultKind::Mcp",
                ],
            ),
            source_check(
                "structured_io_routes_wrappers_without_mutation",
                "crates/mossen-cli/src/structured_io.rs",
                [
                    '"mcp" => match slash_mcp_response(&args).await',
                    '"cost" | "hooks" | "memory" | "skills" | "plugin" | "agents"',
                    "slash_readonly_runtime_inventory_response(&command, &args)",
                    "unsupported_slash_command_args: plugin",
                    '"rawConfigRedacted"',
                    '"mutationSupported"',
                ],
            ),
            source_check(
                "run_all_keeps_w42_registered",
                "scripts/run_all_smoke.sh",
                ["wave_w42_capability_slash_wrappers_smoke.py"],
            ),
        ],
        design_note=(
            "W42 validates the current Rust stream-json slash wrappers for "
            "skills, MCP, plugin, and agents. It replaced removed TS-era "
            "static checks with executable structured_io tests plus manifest/source "
            "checks that prove the wrappers are read-only and redacted."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
