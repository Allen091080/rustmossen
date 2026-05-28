#!/usr/bin/env python3
"""
M15.1 - current Rust full-chain harness smoke.

This smoke validates the current Rust stack with package-local tests that
exercise agent loop status, compaction, context, memory, permissions, skills,
MCP, plugins, and the latest slash/render controls.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions


COMMAND_TIMEOUT_SECS = 240
REAL_HOME = Path(os.environ.get("HOME", str(Path.home())))


@dataclass(frozen=True)
class Step:
    name: str
    area: str
    command: list[str]
    expected: tuple[str, ...] = ()
    timeout_secs: int = COMMAND_TIMEOUT_SECS


def artifact_name(name: str, suffix: str) -> str:
    safe = "".join(ch if ch.isalnum() else "_" for ch in name).strip("_")
    return f"{safe}.{suffix}"


def output_excerpt(text: str, limit: int = 1200) -> str:
    if len(text) <= limit:
        return text
    return text[:limit] + "\n...<truncated>..."


def run_step(ctx: Any, step: Step) -> dict[str, Any]:
    started = time.monotonic()
    timed_out = False
    stdout = ""
    stderr = ""
    exit_code = 1

    try:
        proc = subprocess.run(
            step.command,
            cwd=str(ROOT),
            env=ctx.env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=step.timeout_secs,
        )
        stdout = proc.stdout
        stderr = proc.stderr
        exit_code = proc.returncode
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        stdout = exc.stdout or ""
        stderr = exc.stderr or ""
        exit_code = 124

    duration_secs = round(time.monotonic() - started, 3)
    combined = stdout + "\n" + stderr
    expected_ok = all(token in combined for token in step.expected)
    ok = exit_code == 0 and not timed_out and expected_ok

    stdout_path = ctx.artifacts_dir / artifact_name(step.name, "stdout.txt")
    stderr_path = ctx.artifacts_dir / artifact_name(step.name, "stderr.txt")
    command_path = ctx.artifacts_dir / artifact_name(step.name, "command.txt")
    exit_path = ctx.artifacts_dir / artifact_name(step.name, "exit_code.txt")

    stdout_path.write_text(stdout, encoding="utf-8")
    stderr_path.write_text(stderr, encoding="utf-8")
    command_path.write_text(" ".join(step.command), encoding="utf-8")
    exit_path.write_text(str(exit_code), encoding="utf-8")

    return {
        "name": step.name,
        "area": step.area,
        "ok": ok,
        "exit_code": exit_code,
        "timed_out": timed_out,
        "duration_secs": duration_secs,
        "command": step.command,
        "expected": list(step.expected),
        "expected_ok": expected_ok,
        "stdout_excerpt": output_excerpt(stdout),
        "stderr_excerpt": output_excerpt(stderr),
        "artifacts": {
            "stdout": str(stdout_path),
            "stderr": str(stderr_path),
            "command": str(command_path),
            "exit_code": str(exit_path),
        },
    }


def run_static_checks() -> list[dict[str, Any]]:
    plugin_files = [
        ROOT / "crates/mossen-agent/src/services/plugins/mod.rs",
        ROOT / "crates/mossen-agent/src/services/plugins/cli_commands.rs",
        ROOT / "crates/mossen-agent/src/services/plugins/operations.rs",
        ROOT / "crates/mossen-cli/src/plugin_handlers.rs",
        ROOT / "crates/mossen-commands/src/plugin.rs",
        ROOT / "crates/mossen-utils/src/plugins/marketplace_manager.rs",
    ]
    plugin_missing = [str(path.relative_to(ROOT)) for path in plugin_files if not path.exists()]
    plugin_tokens = {
        "crates/mossen-agent/src/services/plugins/mod.rs": ["cli_commands", "operations"],
        "crates/mossen-cli/src/plugin_handlers.rs": ["plugin"],
        "crates/mossen-commands/src/plugin.rs": ["plugin"],
        "crates/mossen-utils/src/plugins/marketplace_manager.rs": ["marketplace"],
    }
    plugin_token_failures: list[str] = []
    for rel_path, tokens in plugin_tokens.items():
        path = ROOT / rel_path
        if not path.exists():
            continue
        src = path.read_text(encoding="utf-8")
        for token in tokens:
            if token not in src:
                plugin_token_failures.append(f"{rel_path}: missing {token}")

    retired_wrappers = [
        ROOT / ("run-" + "mossen.sh"),
        ROOT / ("run-" + "bun-featured.sh"),
    ]
    missing_retired_wrappers = [
        str(path.relative_to(ROOT)) for path in retired_wrappers if not path.exists()
    ]

    start_mossen = ROOT / "scripts/start-mossen.sh"
    start_mossen_ok = start_mossen.exists() and "target/debug/mossen" in start_mossen.read_text(
        encoding="utf-8"
    )

    return [
        {
            "name": "plugin_rust_surface_present",
            "area": "plugin_system",
            "ok": not plugin_missing and not plugin_token_failures,
            "missing_files": plugin_missing,
            "token_failures": plugin_token_failures,
            "evidence": "plugin Rust service, CLI handler, command, and marketplace surfaces are present",
        },
        {
            "name": "retired_wrapper_status_recorded",
            "area": "harness",
            "ok": bool(missing_retired_wrappers) and start_mossen_ok,
            "missing_retired_wrappers": missing_retired_wrappers,
            "current_rust_runner": "scripts/start-mossen.sh",
            "current_rust_runner_ok": start_mossen_ok,
            "evidence": "retired wrappers are absent, so M15 validates current Rust package tests directly",
        },
    ]


def write_harness_artifacts(ctx: Any, steps: list[dict[str, Any]], checks: list[dict[str, Any]]) -> dict[str, str]:
    report = {
        "test_id": ctx.test_id,
        "fixture_root": str(ctx.root_dir),
        "status": "passed"
        if all(item["ok"] for item in steps) and all(item["ok"] for item in checks)
        else "failed",
        "steps": steps,
        "static_checks": checks,
    }
    report_path = ctx.artifacts_dir / "harness-full-chain-rust-report.json"
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")

    md_lines = [
        "# M15.1 Rust Full-Chain Harness",
        "",
        f"- status: {report['status']}",
        f"- fixture: {ctx.root_dir}",
        "",
        "## Steps",
    ]
    for step in steps:
        marker = "PASS" if step["ok"] else "FAIL"
        md_lines.append(f"- {marker} [{step['area']}] {step['name']} ({step['duration_secs']}s)")
    md_lines.extend(["", "## Static Checks"])
    for check in checks:
        marker = "PASS" if check["ok"] else "FAIL"
        md_lines.append(f"- {marker} [{check['area']}] {check['name']}")
    report_md_path = ctx.artifacts_dir / "harness-full-chain-rust-report.md"
    report_md_path.write_text("\n".join(md_lines) + "\n", encoding="utf-8")

    commands = []
    stdout_summary = []
    stderr_summary = []
    session_lines = []
    for step in steps:
        commands.append(" ".join(step["command"]))
        stdout_summary.append(f"## {step['name']}\n{step['stdout_excerpt']}")
        stderr_summary.append(f"## {step['name']}\n{step['stderr_excerpt']}")
        session_lines.append(json.dumps(step, ensure_ascii=False))
    for check in checks:
        session_lines.append(json.dumps(check, ensure_ascii=False))

    (ctx.artifacts_dir / "command.txt").write_text("\n".join(commands), encoding="utf-8")
    (ctx.artifacts_dir / "stdout.txt").write_text("\n\n".join(stdout_summary), encoding="utf-8")
    (ctx.artifacts_dir / "stderr.txt").write_text("\n\n".join(stderr_summary), encoding="utf-8")
    (ctx.artifacts_dir / "exit_code.txt").write_text(
        "0" if report["status"] == "passed" else "1", encoding="utf-8"
    )
    (ctx.artifacts_dir / "env.txt").write_text(
        "\n".join(
            f"{key}={value}"
            for key, value in sorted(ctx.env.items())
            if key.startswith(("HOME", "MOSSEN_", "XDG_", "RUSTUP_", "CARGO_"))
        ),
        encoding="utf-8",
    )
    (ctx.artifacts_dir / "session_log.jsonl").write_text(
        "\n".join(session_lines) + "\n", encoding="utf-8"
    )

    return {
        "full_chain_report_json": str(report_path),
        "full_chain_report_md": str(report_md_path),
    }


def rust_test(command_tail: list[str]) -> list[str]:
    return ["cargo", "test", "-q", *command_tail, "--", "--nocapture"]


def steps() -> list[Step]:
    cargo_ok = ("test result: ok.", "1 passed;")
    return [
        Step(
            "agent_loop_runtime_status",
            "agent_loop",
            rust_test([
                "-p",
                "mossen-agent",
                "--lib",
                "services::root::runtime_status::tests::runtime_status_tracks_tool_and_permission_decisions",
            ]),
            cargo_ok,
        ),
        Step(
            "agent_loop_tool_cancel",
            "agent_loop",
            rust_test([
                "-p",
                "mossen-agent",
                "--lib",
                "tool_registry::tests::execute_with_cancel_drops_in_flight_tool_future",
            ]),
            cargo_ok,
        ),
        Step(
            "context_compact_boundary",
            "context_compaction",
            rust_test([
                "-p",
                "mossen-agent",
                "--lib",
                "dialogue::tests::pending_compact_request_compacts_state_and_emits_boundary",
            ]),
            cargo_ok,
        ),
        Step(
            "context_memory_compact",
            "context_compaction",
            rust_test([
                "-p",
                "mossen-agent",
                "--lib",
                "services::compact::session_memory_compact::tests::session_memory_compaction_uses_memory_and_preserves_recent_messages",
            ]),
            cargo_ok,
        ),
        Step(
            "context_slash_snapshot",
            "context_management",
            rust_test([
                "-p",
                "mossen-cli",
                "--bin",
                "mossen",
                "structured_io::tests::slash_command_context_reports_token_window_snapshot",
            ]),
            cargo_ok,
        ),
        Step(
            "memory_extract_prompt",
            "memory_system",
            rust_test([
                "-p",
                "mossen-agent",
                "--lib",
                "services::extract_memories::tests::extraction_prompt_uses_auto_only_memory_by_default",
            ]),
            cargo_ok,
        ),
        Step(
            "permissions_agent_rules",
            "permission_system",
            rust_test([
                "-p",
                "mossen-agent",
                "--lib",
                "dialogue::tests::session_permission_rules_deny_precedes_allow",
            ]),
            cargo_ok,
        ),
        Step(
            "permissions_cli_picker",
            "permission_system",
            rust_test([
                "-p",
                "mossen-cli",
                "--bin",
                "mossen",
                "structured_io::tests::slash_command_permissions_reports_current_mode",
            ]),
            cargo_ok,
        ),
        Step(
            "skill_prompt_inventory",
            "skill_system",
            rust_test([
                "-p",
                "mossen-cli",
                "--bin",
                "mossen",
                "system_prompt::tests::assemble_includes_loaded_skill_inventory_when_skill_tool_enabled",
            ]),
            cargo_ok,
        ),
        Step(
            "skill_dynamic_discovery",
            "skill_system",
            rust_test([
                "-p",
                "mossen-skills",
                "--lib",
                "dynamic::tests::discover_skill_dirs_checks_cwd_level_skills",
            ]),
            cargo_ok,
        ),
        Step(
            "skill_startup_user_project_sources",
            "skill_system",
            rust_test([
                "-p",
                "mossen-skills",
                "--lib",
                "dynamic::tests::startup_loads_user_and_project_skill_sources",
            ]),
            cargo_ok,
        ),
        Step(
            "skill_reload_updates_content",
            "skill_system",
            rust_test([
                "-p",
                "mossen-skills",
                "--lib",
                "dynamic::tests::add_skill_directories_reload_updates_existing_skill_content",
            ]),
            cargo_ok,
        ),
        Step(
            "skill_error_isolation",
            "skill_system",
            rust_test([
                "-p",
                "mossen-skills",
                "--lib",
                "dynamic::tests::load_skills_from_dir_skips_bad_entry_and_keeps_good_skill",
            ]),
            cargo_ok,
        ),
        Step(
            "skill_tool_execution",
            "skill_system",
            rust_test([
                "-p",
                "mossen-tools",
                "--lib",
                "skill::tests::skill_tool_executes_loaded_dynamic_skill",
            ]),
            cargo_ok,
        ),
        Step(
            "skill_tool_followup_body",
            "skill_system",
            rust_test([
                "-p",
                "mossen-tools",
                "--lib",
                "skill::tests::skill_tool_result_contains_rendered_body_for_model_followup",
            ]),
            cargo_ok,
        ),
        Step(
            "skill_slash_inventory",
            "skill_system",
            rust_test([
                "-p",
                "mossen-cli",
                "--bin",
                "mossen",
                "structured_io::tests::slash_command_skills_lists_available_inventory_redacted",
            ]),
            cargo_ok,
        ),
        Step(
            "mcp_tool_definition",
            "mcp_system",
            rust_test([
                "-p",
                "mossen-mcp",
                "--lib",
                "tools::tests::converts_mcp_tool_to_model_visible_definition",
            ]),
            cargo_ok,
        ),
        Step(
            "mcp_config_toggle",
            "mcp_system",
            rust_test([
                "-p",
                "mossen-mcp",
                "--lib",
                "config_ext::tests::set_mcp_server_enabled_round_trip",
            ]),
            cargo_ok,
        ),
        Step(
            "mcp_slash_inventory",
            "mcp_system",
            rust_test([
                "-p",
                "mossen-cli",
                "--bin",
                "mossen",
                "structured_io::tests::slash_command_mcp_inventory_returns_redacted_snapshot",
            ]),
            cargo_ok,
        ),
        Step(
            "plugin_marketplace_redaction",
            "plugin_system",
            rust_test([
                "-p",
                "mossen-utils",
                "--lib",
                "plugins::marketplace_manager::tests::test_redact_url_credentials",
            ]),
            cargo_ok,
        ),
        Step(
            "plugin_agent_operations",
            "plugin_system",
            rust_test([
                "-p",
                "mossen-agent",
                "--lib",
                "services::plugins::operations::tests::plugin_install_enable_disable_uninstall_updates_settings",
            ]),
            cargo_ok,
        ),
        Step(
            "plugin_policy_block",
            "plugin_system",
            rust_test([
                "-p",
                "mossen-agent",
                "--lib",
                "services::plugins::operations::tests::plugin_policy_block_prevents_install_and_enable",
            ]),
            cargo_ok,
        ),
        Step(
            "plugin_slash_directive",
            "plugin_system",
            rust_test([
                "-p",
                "mossen-commands",
                "--lib",
                "plugin::tests::plugin_directive_routes_core_actions",
            ]),
            cargo_ok,
        ),
        Step(
            "slash_compact_permissions_controls",
            "slash_controls",
            ["python3", "scripts/wave_w323_stream_json_slash_compact_permissions_controls_smoke.py"],
            ("wave_w323_stream_json_slash_compact_permissions_controls_smoke: ok",),
            timeout_secs=60,
        ),
        Step(
            "tui_composer_final_summary_noise",
            "terminal_rendering",
            ["python3", "scripts/wave_w324_tui_composer_final_summary_noise_smoke.py"],
            ("wave_w324_tui_composer_final_summary_noise_smoke: ok",),
            timeout_secs=60,
        ),
    ]


def main() -> int:
    ctx = make_fixture("M15.1")
    ctx.env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    ctx.env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    results = [run_step(ctx, step) for step in steps()]
    checks = run_static_checks()
    extra_artifacts = write_harness_artifacts(ctx, results, checks)

    all_ok = all(item["ok"] for item in results) and all(item["ok"] for item in checks)
    assertions = [
        {
            "name": item["name"],
            "area": item["area"],
            "expected": True,
            "actual": item["ok"],
            "passed": item["ok"],
            "evidence": (
                f"exit={item['exit_code']} timeout={item['timed_out']} "
                f"duration={item['duration_secs']}s"
            ),
        }
        for item in results
    ]
    assertions.extend(
        {
            "name": item["name"],
            "area": item["area"],
            "expected": True,
            "actual": item["ok"],
            "passed": item["ok"],
            "evidence": item["evidence"],
        }
        for item in checks
    )

    write_assertions(
        ctx,
        status="passed" if all_ok else "failed",
        assertions=assertions,
        extra_artifacts=extra_artifacts,
    )

    summary = {
        "test_id": ctx.test_id,
        "status": "passed" if all_ok else "failed",
        "fixture_root": str(ctx.root_dir),
        "passed": sum(1 for item in results if item["ok"]) + sum(1 for item in checks if item["ok"]),
        "total": len(results) + len(checks),
        "failed": [
            item["name"]
            for item in [*results, *checks]
            if not item["ok"]
        ],
        "report": extra_artifacts["full_chain_report_json"],
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
