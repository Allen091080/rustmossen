#!/usr/bin/env python3
"""R11.1 - production release readiness gate contract.

This is not a substitute for the credentialed provider soaks or interactive
TTY/coding bakes. It proves those gates are first-class required blockers with
commands, evidence paths, and pass criteria, so release readiness cannot be
claimed from the deterministic harness alone.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log
from release_provider_long_soak import write_profile_template
from release_readiness_status import (
    blocked_gate_rows,
    coding_sprint_status,
    evidence_freshness_status,
    freshness_baselines,
    harness_evidence_source_files,
    manual_tty_bake_status,
    provider_soak_status,
)


READINESS_PATH = ROOT / "harness" / "release_readiness.v1.json"

EXPECTED_REQUIRED_GATES = {
    "deterministic_harness_matrix",
    "rust_workspace_tests",
    "warnings_as_errors",
    "package_install_smoke",
    "external_provider_openai_compatible_long_soak",
    "external_provider_anthropic_long_soak",
    "external_provider_openai_responses_long_soak",
    "pty_30_min_soak",
    "manual_tty_30_min_bake",
    "one_hour_coding_sprint",
}

EXPECTED_PROTOCOLS = {
    "external_provider_openai_compatible_long_soak": "openai-compatible",
    "external_provider_anthropic_long_soak": "anthropic",
    "external_provider_openai_responses_long_soak": "openai-responses",
}

EXPECTED_PROVIDER_SCENARIOS = {
    "streaming",
    "tool_call",
    "retry",
    "cancel",
    "compact",
    "subagent",
    "resume",
}

EXPECTED_PACKAGE_CRITERIA_TERMS = {
    "release build",
    "PATH",
    "outside the source repo",
    "first launch",
    "startup latency",
    "profile migration",
    "redact",
}

EXPECTED_CODING_SPRINT_CHECKS = {
    "background_agent_used",
    "taskoutput_retrieved",
    "file_edit_performed",
    "bash_command_run",
    "interrupt_or_resume_exercised",
    "final_summary_recorded",
    "tasks_finished_without_turn_limit",
    "per_task_validation_bash_succeeded",
    "input_responsive_during_agent_work",
    "input_responsiveness_evidence_recorded",
    "background_agent_completion_surfaced",
    "final_summary_matches_activity",
}

EXPECTED_MANUAL_TTY_CHECKS = {
    "input_responsive",
    "selection_copy_works",
    "scroll_returns_to_bottom",
    "subagent_completion_feedback",
    "ctrl_c_safe",
    "no_panic_or_deadlock",
    "no_render_tearing",
}


def ok_assertion(name: str, ok: bool, **detail: Any) -> dict[str, Any]:
    return {"name": name, "ok": ok, **detail}


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")


def write_synthetic_coding_sprint_evidence(ctx) -> dict[str, Any]:
    sprint_dir = ctx.artifacts_dir / "synthetic-coding-sprint"
    sprint_dir.mkdir(parents=True, exist_ok=True)
    transcript_path = sprint_dir / "transcript.jsonl"
    summary_path = sprint_dir / "summary.json"
    input_path = sprint_dir / "input-responsive-evidence.json"

    def append(payload: dict[str, Any]) -> None:
        with transcript_path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(payload, ensure_ascii=False) + "\n")

    append({"type": "runner_task_start", "task_id": "synthetic", "command": ["mossen", "--restore-id", "abc"]})
    append({"type": "tool_use_summary", "runner_task_id": "synthetic", "tool_name": "Agent", "summary": "async_launched"})
    append({"type": "tool_use_summary", "runner_task_id": "synthetic", "tool_name": "TaskOutput", "summary": "completed"})
    append({"type": "tool_use_summary", "runner_task_id": "synthetic", "tool_name": "Write", "summary": "wrote sprintcalc/core.py"})
    append(
        {
            "type": "assistant",
            "runner_task_id": "synthetic",
            "message": {
                "content": [
                    {
                        "type": "tool_use",
                        "id": "synthetic-bash-failed",
                        "name": "Bash",
                        "input": {
                            "command": "PYTHONPATH=. python3 -m pytest tests/ -v 2>&1 | tail -20"
                        },
                    }
                ]
            },
        }
    )
    append(
        {
            "type": "tool_use_summary",
            "runner_task_id": "synthetic",
            "tool_name": "Bash",
            "tool_use_id": "synthetic-bash-failed",
            "summary": '{"stdout":"FAILED tests/test_core.py::test_example\\n1 failed\\n","exit_code":0}',
        }
    )
    append(
        {
            "type": "assistant",
            "runner_task_id": "synthetic",
            "message": {
                "content": [
                    {
                        "type": "tool_use",
                        "id": "synthetic-bash-passed",
                        "name": "Bash",
                        "input": {
                            "command": "PYTHONPATH=. python3 -m pytest tests/ -v 2>&1 | tail -20"
                        },
                    }
                ]
            },
        }
    )
    append(
        {
            "type": "tool_use_summary",
            "runner_task_id": "synthetic",
            "tool_name": "Bash",
            "tool_use_id": "synthetic-bash-passed",
            "summary": '{"stdout":"tests/test_core.py::test_example PASSED [100%]\\n1 passed in 0.01s\\n","exit_code":0}',
        }
    )
    append({"type": "assistant", "message": {"content": "Final Summary\nFiles: sprintcalc/core.py\nCommands: pytest"}})
    append({"type": "runner_task_finish", "task_id": "synthetic", "status": "completed", "exit_code": 0})

    write_json(
        input_path,
        {
            "ok": True,
            "method": "synthetic_contract_smoke",
            "observed_at": "2026-05-28T00:00:00Z",
            "during_background_agent_work": True,
            "observations": [
                {
                    "action": "typed while a background Agent was registered",
                    "result": "input event was accepted without blocking the harness contract",
                }
            ],
        },
    )
    write_json(
        summary_path,
        {
            "ok": True,
            "duration_minutes": 0.1,
            "tasks_completed": 1,
            "tasks": [{"id": "synthetic", "status": "completed", "terminal": "Completed"}],
            "checks": {check: True for check in EXPECTED_CODING_SPRINT_CHECKS},
            "input_responsiveness_evidence": str(input_path),
        },
    )
    gate = {
        "id": "one_hour_coding_sprint",
        "evidence": [str(transcript_path), str(summary_path), str(input_path)],
        "minimum_duration_minutes": 0.01,
        "minimum_tasks": 1,
    }
    status, reason = coding_sprint_status(gate)
    return {
        "status": status,
        "reason": reason,
        "transcript": str(transcript_path),
        "summary": str(summary_path),
        "input_evidence": str(input_path),
    }


def write_mismatched_coding_sprint_evidence(ctx) -> dict[str, Any]:
    sprint_dir = ctx.artifacts_dir / "mismatched-coding-sprint"
    sprint_dir.mkdir(parents=True, exist_ok=True)
    transcript_path = sprint_dir / "transcript.jsonl"
    summary_path = sprint_dir / "summary.json"
    input_path = sprint_dir / "input-responsive-evidence.json"

    def append(payload: dict[str, Any]) -> None:
        with transcript_path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(payload, ensure_ascii=False) + "\n")

    append({"type": "runner_task_start", "task_id": "edited", "command": ["mossen"]})
    append({"type": "tool_use_summary", "runner_task_id": "edited", "tool_name": "Agent", "summary": "async_launched"})
    append({"type": "tool_use_summary", "runner_task_id": "edited", "tool_name": "TaskOutput", "summary": "completed"})
    append({"type": "tool_use_summary", "runner_task_id": "edited", "tool_name": "Write", "summary": "wrote file"})
    append({"type": "tool_use_summary", "runner_task_id": "edited", "tool_name": "Bash", "summary": "pytest test session starts\n1 passed\nexit 0"})
    append({"type": "assistant", "runner_task_id": "edited", "message": {"content": "Final Summary\nChanged files:\n- sprintcalc/core.py"}})
    append({"type": "runner_task_finish", "task_id": "edited", "status": "completed", "exit_code": 0})
    append({"type": "runner_task_start", "task_id": "false-summary", "command": ["mossen", "--restore-id", "abc"]})
    append({"type": "tool_use_summary", "runner_task_id": "false-summary", "tool_name": "Read", "summary": "read file"})
    append({"type": "assistant", "runner_task_id": "false-summary", "message": {"content": "Final Summary\nChanged files:\n- sprintcalc/cli.py"}})
    append({"type": "runner_task_finish", "task_id": "false-summary", "status": "completed", "exit_code": 0})

    write_json(
        input_path,
        {
            "ok": True,
            "method": "synthetic_contract_smoke",
            "observed_at": "2026-05-28T00:00:00Z",
            "during_background_agent_work": True,
            "observations": [{"action": "typed", "result": "accepted"}],
        },
    )
    write_json(
        summary_path,
        {
            "ok": True,
            "duration_minutes": 0.1,
            "tasks_completed": 2,
            "tasks": [
                {"id": "edited", "status": "completed", "terminal": "Completed"},
                {"id": "false-summary", "status": "completed", "terminal": "Completed"},
            ],
            "checks": {check: True for check in EXPECTED_CODING_SPRINT_CHECKS},
            "input_responsiveness_evidence": str(input_path),
        },
    )
    gate = {
        "id": "one_hour_coding_sprint",
        "evidence": [str(transcript_path), str(summary_path), str(input_path)],
        "minimum_duration_minutes": 0.01,
        "minimum_tasks": 1,
    }
    status, reason = coding_sprint_status(gate)
    return {"status": status, "reason": reason}


def provider_missing_profile_diagnostic(ctx) -> dict[str, Any]:
    config_path = ctx.artifacts_dir / "profile-diagnostic-settings.json"
    write_json(
        config_path,
        {
            "mossen.profiles": {
                "example": {
                    "provider": "openai-compatible",
                    "baseURL": "https://api.example.com/v1",
                    "model": "example-highspeed",
                    "apiKey": "redacted-test-key",
                }
            }
        },
    )
    previous = os.environ.get("MOSSEN_RELEASE_PROFILE_CONFIG")
    os.environ["MOSSEN_RELEASE_PROFILE_CONFIG"] = str(config_path)
    try:
        status, reason = provider_soak_status(
            {
                "id": "external_provider_anthropic_long_soak",
                "protocol": "anthropic",
                "command": "python3 scripts/release_provider_long_soak.py --profile anthropic --minutes 30",
                "evidence": [str(ctx.artifacts_dir / "missing-anthropic-soak.json")],
                "minimum_duration_minutes": 30,
                "required_scenarios": sorted(EXPECTED_PROVIDER_SCENARIOS),
            }
        )
    finally:
        if previous is None:
            os.environ.pop("MOSSEN_RELEASE_PROFILE_CONFIG", None)
        else:
            os.environ["MOSSEN_RELEASE_PROFILE_CONFIG"] = previous
    return {"status": status, "reason": reason}


def manual_tty_missing_evidence_diagnostic(ctx) -> dict[str, Any]:
    status, reason = manual_tty_bake_status(
        {
            "id": "manual_tty_30_min_bake",
            "command": "python3 scripts/release_manual_tty_bake.py --record && python3 scripts/release_manual_tty_bake.py",
            "evidence": [str(ctx.artifacts_dir / "missing-manual-tty-evidence.json")],
            "minimum_duration_minutes": 30,
            "required_checks": sorted(EXPECTED_MANUAL_TTY_CHECKS),
        }
    )
    return {"status": status, "reason": reason}


def manual_tty_unfilled_template_diagnostic(ctx) -> dict[str, Any]:
    evidence_path = ctx.artifacts_dir / "unfilled-manual-tty-evidence.json"
    write_json(
        evidence_path,
        {
            "schema_version": 1,
            "ok": False,
            "started_at": "",
            "ended_at": "",
            "duration_minutes": 30,
            "terminal_app": "",
            "mossen_command": "./target/release/mossen",
            "checks": {name: False for name in EXPECTED_MANUAL_TTY_CHECKS},
        },
    )
    status, reason = manual_tty_bake_status(
        {
            "id": "manual_tty_30_min_bake",
            "command": "python3 scripts/release_manual_tty_bake.py --record && python3 scripts/release_manual_tty_bake.py",
            "evidence": [str(evidence_path)],
            "minimum_duration_minutes": 30,
            "required_checks": sorted(EXPECTED_MANUAL_TTY_CHECKS),
        }
    )
    return {"status": status, "reason": reason}


def manual_tty_record_rejects_non_tty(ctx) -> dict[str, Any]:
    evidence_path = ctx.artifacts_dir / "non-tty-record-evidence.json"
    proc = subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts" / "release_manual_tty_bake.py"),
            "--record",
            "--evidence",
            str(evidence_path),
        ],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        check=False,
    )
    return {
        "exit_code": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "evidence_exists": evidence_path.exists(),
    }


def manual_tty_template_does_not_create_evidence(ctx) -> dict[str, Any]:
    evidence_path = ctx.artifacts_dir / "manual-tty-template-must-not-be-evidence.json"
    template_path = ctx.artifacts_dir / "manual-tty-evidence.template.json"
    proc = subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts" / "release_manual_tty_bake.py"),
            "--write-template",
            "--evidence",
            str(evidence_path),
            "--template",
            str(template_path),
        ],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        check=False,
    )
    payload: dict[str, Any] = {}
    try:
        payload = json.loads(proc.stdout)
    except json.JSONDecodeError:
        pass
    return {
        "exit_code": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "template_path": str(template_path),
        "template_exists": template_path.exists(),
        "evidence_path": str(evidence_path),
        "evidence_exists": evidence_path.exists(),
        "reported_template": payload.get("template"),
    }


def write_provider_profile_template_evidence(ctx) -> dict[str, Any]:
    template_path = ctx.artifacts_dir / "provider-profiles.template.json"
    payload = write_profile_template(template_path)
    profiles = payload.get("mossen.profiles") or {}
    return {
        "path": str(template_path),
        "profile_names": sorted(profiles),
        "anthropic_provider": profiles.get("anthropic-release", {}).get("provider"),
        "responses_provider": profiles.get("openai-responses-release", {}).get("provider"),
        "anthropic_api_key": profiles.get("anthropic-release", {}).get("apiKey"),
        "responses_api_key": profiles.get("openai-responses-release", {}).get("apiKey"),
        "commands": payload.get("_commands") or {},
    }


def provider_dry_run(ctx, protocol: str) -> dict[str, Any]:
    secret = f"dry-run-secret-{protocol}"
    proc = subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts" / "release_provider_long_soak.py"),
            "--dry-run",
            "--protocol",
            protocol,
            "--base-url",
            f"https://example.invalid/{protocol}",
            "--model",
            f"{protocol}-model",
            "--api-key",
            secret,
            "--artifact-root",
            str(ctx.artifacts_dir / "provider-dry-run"),
        ],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        check=False,
    )
    payload: dict[str, Any] = {}
    try:
        payload = json.loads(proc.stdout)
    except json.JSONDecodeError:
        pass
    plan = payload.get("plan") if isinstance(payload.get("plan"), dict) else {}
    return {
        "protocol": protocol,
        "exit_code": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "dry_run": payload.get("dry_run"),
        "plan_protocol": plan.get("protocol"),
        "plan_model": plan.get("model"),
        "plan_api_key": plan.get("api_key"),
        "plan_scenarios": sorted(plan.get("scenarios") or []),
        "secret_leaked": secret in proc.stdout or secret in proc.stderr,
    }


def freshness_diagnostic(ctx) -> dict[str, Any]:
    source = ctx.artifacts_dir / "freshness-source.rs"
    stale_evidence = ctx.artifacts_dir / "stale-evidence.log"
    fresh_evidence = ctx.artifacts_dir / "fresh-evidence.log"
    source.write_text("fn main() {}\n", encoding="utf-8")
    stale_evidence.write_text("test result: ok\n", encoding="utf-8")
    fresh_evidence.write_text("test result: ok\n", encoding="utf-8")
    source_time = source.stat().st_mtime
    os.utime(stale_evidence, (source_time - 10, source_time - 10))
    os.utime(fresh_evidence, (source_time + 10, source_time + 10))
    stale_ok, stale_reason = evidence_freshness_status(
        [stale_evidence], source_time, "synthetic source"
    )
    fresh_ok, fresh_reason = evidence_freshness_status(
        [fresh_evidence], source_time, "synthetic source"
    )
    return {
        "stale_ok": stale_ok,
        "stale_reason": stale_reason,
        "fresh_ok": fresh_ok,
        "fresh_reason": fresh_reason,
    }


def main() -> int:
    ctx = make_fixture("R11.1_release_readiness_contract")
    raw = READINESS_PATH.read_text(encoding="utf-8")
    payload = json.loads(raw)
    gates = payload.get("gates", [])
    by_id = {gate.get("id"): gate for gate in gates}

    assertions: list[dict[str, Any]] = []
    assertions.append(
        ok_assertion(
            "schema_version_is_v1",
            payload.get("schema_version") == 1,
            schema_version=payload.get("schema_version"),
        )
    )
    assertions.append(
        ok_assertion(
            "decision_rule_requires_all_required_gates",
            "every required gate" in payload.get("decision_rule", ""),
            decision_rule=payload.get("decision_rule", ""),
        )
    )

    missing = sorted(EXPECTED_REQUIRED_GATES - set(by_id))
    assertions.append(
        ok_assertion(
            "all_release_blocker_gates_declared",
            not missing,
            missing=missing,
            declared=sorted(by_id),
        )
    )

    required_not_true = sorted(
        gate_id for gate_id in EXPECTED_REQUIRED_GATES if not by_id.get(gate_id, {}).get("required")
    )
    assertions.append(
        ok_assertion(
            "expected_gates_are_required",
            not required_not_true,
            required_not_true=required_not_true,
        )
    )

    missing_evidence_or_criteria = sorted(
        gate_id
        for gate_id in EXPECTED_REQUIRED_GATES
        if not by_id.get(gate_id, {}).get("evidence")
        or not by_id.get(gate_id, {}).get("pass_criteria")
    )
    assertions.append(
        ok_assertion(
            "required_gates_have_evidence_and_pass_criteria",
            not missing_evidence_or_criteria,
            missing=missing_evidence_or_criteria,
        )
    )

    protocol_mismatches = {
        gate_id: by_id.get(gate_id, {}).get("protocol")
        for gate_id, expected in EXPECTED_PROTOCOLS.items()
        if by_id.get(gate_id, {}).get("protocol") != expected
    }
    assertions.append(
        ok_assertion(
            "future_provider_protocols_are_explicit_release_gates",
            not protocol_mismatches,
            protocol_mismatches=protocol_mismatches,
        )
    )
    scenario_mismatches = {
        gate_id: sorted(set(by_id.get(gate_id, {}).get("required_scenarios") or []))
        for gate_id in EXPECTED_PROTOCOLS
        if set(by_id.get(gate_id, {}).get("required_scenarios") or [])
        != EXPECTED_PROVIDER_SCENARIOS
    }
    assertions.append(
        ok_assertion(
            "provider_long_soaks_require_full_runtime_scenarios",
            not scenario_mismatches,
            scenario_mismatches=scenario_mismatches,
        )
    )

    minimax_gate = by_id.get("external_provider_openai_compatible_long_soak", {})
    assertions.append(
        ok_assertion(
            "minimax_openai_compatible_profile_is_required",
            minimax_gate.get("reference_profile") == "example-fast-highspeed",
            reference_profile=minimax_gate.get("reference_profile"),
        )
    )

    package_gate = by_id.get("package_install_smoke", {})
    package_criteria = " ".join(package_gate.get("pass_criteria") or [])
    missing_package_terms = sorted(
        term for term in EXPECTED_PACKAGE_CRITERIA_TERMS if term not in package_criteria
    )
    assertions.append(
        ok_assertion(
            "package_gate_covers_production_install_shape",
            not missing_package_terms,
            missing_terms=missing_package_terms,
        )
    )

    pty_gate = by_id.get("pty_30_min_soak", {})
    assertions.append(
        ok_assertion(
            "pty_soak_requires_thirty_minutes",
            pty_gate.get("minimum_duration_minutes") == 30,
            minimum_duration_minutes=pty_gate.get("minimum_duration_minutes"),
        )
    )

    manual_tty_gate = by_id.get("manual_tty_30_min_bake", {})
    manual_tty_checks = set(manual_tty_gate.get("required_checks") or [])
    manual_tty_criteria = " ".join(manual_tty_gate.get("pass_criteria") or [])
    assertions.append(
        ok_assertion(
            "manual_tty_bake_is_not_replaced_by_pty_soak",
            manual_tty_gate.get("minimum_duration_minutes") == 30
            and manual_tty_checks == EXPECTED_MANUAL_TTY_CHECKS,
            minimum_duration_minutes=manual_tty_gate.get("minimum_duration_minutes"),
            missing=sorted(EXPECTED_MANUAL_TTY_CHECKS - manual_tty_checks),
            extra=sorted(manual_tty_checks - EXPECTED_MANUAL_TTY_CHECKS),
        )
    )
    assertions.append(
        ok_assertion(
            "manual_tty_bake_requires_selection_copy_and_feedback",
            "selection and copy" in manual_tty_criteria
            and "background Agent completion" in manual_tty_criteria
            and "Ctrl+C" in manual_tty_criteria,
            pass_criteria=manual_tty_gate.get("pass_criteria"),
        )
    )

    coding_gate = by_id.get("one_hour_coding_sprint", {})
    coding_checks = set(coding_gate.get("required_checks") or [])
    assertions.append(
        ok_assertion(
            "coding_sprint_requires_five_real_tasks",
            coding_gate.get("minimum_tasks") == 5,
            minimum_tasks=coding_gate.get("minimum_tasks"),
        )
    )
    assertions.append(
        ok_assertion(
            "coding_sprint_requires_runtime_chain_checks",
            coding_checks == EXPECTED_CODING_SPRINT_CHECKS,
            missing=sorted(EXPECTED_CODING_SPRINT_CHECKS - coding_checks),
            extra=sorted(coding_checks - EXPECTED_CODING_SPRINT_CHECKS),
        )
    )
    coding_criteria = " ".join(coding_gate.get("pass_criteria") or [])
    assertions.append(
        ok_assertion(
            "coding_sprint_gate_uses_audit_script",
            "release_coding_sprint_audit.py" in coding_criteria
            and "release_coding_sprint_audit.py" in coding_gate.get("command", ""),
            command=coding_gate.get("command"),
        )
    )
    assertions.append(
        ok_assertion(
            "coding_sprint_gate_uses_runner_script",
            "release_coding_sprint_runner.py" in coding_gate.get("command", ""),
            command=coding_gate.get("command"),
        )
    )

    manual_or_credentialed = [
        gate
        for gate in gates
        if gate.get("kind") in {"credentialed", "interactive"} and gate.get("required")
    ]
    assertions.append(
        ok_assertion(
            "credentialed_and_interactive_gates_remain_blocking",
            len(manual_or_credentialed) >= 5,
            blocking_gate_ids=sorted(gate.get("id") for gate in manual_or_credentialed),
        )
    )
    assertions.append(
        ok_assertion(
            "readiness_status_evaluator_exists",
            (ROOT / "scripts" / "release_readiness_status.py").exists(),
            evaluator="scripts/release_readiness_status.py",
        )
    )
    assertions.append(
        ok_assertion(
            "coding_sprint_audit_exists",
            (ROOT / "scripts" / "release_coding_sprint_audit.py").exists(),
            evaluator="scripts/release_coding_sprint_audit.py",
        )
    )
    assertions.append(
        ok_assertion(
            "coding_sprint_runner_exists",
            (ROOT / "scripts" / "release_coding_sprint_runner.py").exists(),
            runner="scripts/release_coding_sprint_runner.py",
        )
    )
    synthetic_sprint_status = write_synthetic_coding_sprint_evidence(ctx)
    assertions.append(
        ok_assertion(
            "readiness_status_uses_summary_json_not_last_evidence",
            synthetic_sprint_status["status"] == "passed",
            status=synthetic_sprint_status["status"],
            reason=synthetic_sprint_status["reason"],
            summary=synthetic_sprint_status["summary"],
            input_evidence=synthetic_sprint_status["input_evidence"],
        )
    )
    mismatched_sprint_status = write_mismatched_coding_sprint_evidence(ctx)
    assertions.append(
        ok_assertion(
            "coding_sprint_rejects_false_changed_files_summary",
            mismatched_sprint_status["status"] == "failed"
            and "final_summaries_match_task_activity" in mismatched_sprint_status["reason"],
            status=mismatched_sprint_status["status"],
            reason=mismatched_sprint_status["reason"],
        )
    )
    missing_provider_diagnostic = provider_missing_profile_diagnostic(ctx)
    assertions.append(
        ok_assertion(
            "provider_soak_missing_report_points_to_missing_protocol_profile",
            missing_provider_diagnostic["status"] == "missing"
            and "no configured anthropic profile" in missing_provider_diagnostic["reason"]
            and "release_provider_long_soak.py --write-profile-template" in missing_provider_diagnostic["reason"]
            and "openai-compatible: minimax" in missing_provider_diagnostic["reason"]
            and "redacted-test-key" not in missing_provider_diagnostic["reason"],
            status=missing_provider_diagnostic["status"],
            reason=missing_provider_diagnostic["reason"],
        )
    )
    manual_tty_diagnostic = manual_tty_missing_evidence_diagnostic(ctx)
    assertions.append(
        ok_assertion(
            "manual_tty_missing_evidence_points_to_template_command",
            manual_tty_diagnostic["status"] == "missing"
            and "release_manual_tty_bake.py --record" in manual_tty_diagnostic["reason"],
            status=manual_tty_diagnostic["status"],
            reason=manual_tty_diagnostic["reason"],
        )
    )
    manual_tty_unfilled_template = manual_tty_unfilled_template_diagnostic(ctx)
    assertions.append(
        ok_assertion(
            "manual_tty_unfilled_template_is_reported_as_missing_evidence",
            manual_tty_unfilled_template["status"] == "missing"
            and "unfilled template" in manual_tty_unfilled_template["reason"]
            and "release_manual_tty_bake.py --record" in manual_tty_unfilled_template["reason"],
            status=manual_tty_unfilled_template["status"],
            reason=manual_tty_unfilled_template["reason"],
        )
    )
    non_tty_record = manual_tty_record_rejects_non_tty(ctx)
    assertions.append(
        ok_assertion(
            "manual_tty_record_refuses_non_tty_execution",
            non_tty_record["exit_code"] != 0
            and "real interactive TTY" in non_tty_record["stdout"]
            and not non_tty_record["evidence_exists"],
            exit_code=non_tty_record["exit_code"],
            stdout=non_tty_record["stdout"],
            stderr=non_tty_record["stderr"],
            evidence_exists=non_tty_record["evidence_exists"],
        )
    )
    manual_tty_template = manual_tty_template_does_not_create_evidence(ctx)
    assertions.append(
        ok_assertion(
            "manual_tty_template_does_not_pollute_evidence_path",
            manual_tty_template["exit_code"] == 0
            and manual_tty_template["template_exists"]
            and not manual_tty_template["evidence_exists"]
            and manual_tty_template["reported_template"] == manual_tty_template["template_path"],
            result=manual_tty_template,
        )
    )
    provider_template = write_provider_profile_template_evidence(ctx)
    assertions.append(
        ok_assertion(
            "provider_profile_template_covers_missing_protocols_without_secrets",
            provider_template["anthropic_provider"] == "anthropic"
            and provider_template["responses_provider"] == "openai-responses"
            and provider_template["anthropic_api_key"] == ""
            and provider_template["responses_api_key"] == ""
            and "anthropic-release" in provider_template["commands"].get("anthropic", "")
            and "openai-responses-release" in provider_template["commands"].get("openai-responses", ""),
            template=provider_template["path"],
            profile_names=provider_template["profile_names"],
            commands=provider_template["commands"],
        )
    )
    dry_runs = {
        protocol: provider_dry_run(ctx, protocol)
        for protocol in sorted({"openai-compatible", "anthropic", "openai-responses"})
    }
    assertions.append(
        ok_assertion(
            "provider_long_soak_dry_run_covers_all_release_protocols_without_secret_leaks",
            all(item["exit_code"] == 0 for item in dry_runs.values())
            and all(item["dry_run"] is True for item in dry_runs.values())
            and all(protocol == item["plan_protocol"] for protocol, item in dry_runs.items())
            and all(item["plan_api_key"] == "<redacted>" for item in dry_runs.values())
            and all(not item["secret_leaked"] for item in dry_runs.values())
            and all(
                set(item["plan_scenarios"]) == EXPECTED_PROVIDER_SCENARIOS
                for item in dry_runs.values()
            ),
            dry_runs=dry_runs,
        )
    )
    freshness = freshness_diagnostic(ctx)
    assertions.append(
        ok_assertion(
            "readiness_status_rejects_stale_automated_evidence",
            freshness["stale_ok"] is False
            and "stale evidence" in freshness["stale_reason"]
            and freshness["fresh_ok"] is True,
            freshness=freshness,
        )
    )
    harness_sources = [path for path in harness_evidence_source_files() if path.exists()]
    assertions.append(
        ok_assertion(
            "deterministic_harness_freshness_tracks_matrix_and_harness_scripts",
            (ROOT / "harness" / "capability_matrix.v1.json") in harness_sources
            and (ROOT / "scripts" / "harness_assertions_aggregator.py") in harness_sources
            and (ROOT / "scripts" / "harness_capability_matrix.py") in harness_sources
            and any(path.name.startswith("harness_R11_") for path in harness_sources),
            source_count=len(harness_sources),
        )
    )
    baselines = freshness_baselines()
    assertions.append(
        ok_assertion(
            "readiness_status_reports_freshness_baselines",
            bool(baselines.get("rust_source", {}).get("latest_file"))
            and bool(baselines.get("harness_sources", {}).get("latest_file"))
            and baselines.get("rust_source", {}).get("file_count", 0) > 0
            and baselines.get("harness_sources", {}).get("file_count", 0) > 0,
            baselines=baselines,
        )
    )
    synthetic_blockers = blocked_gate_rows(
        [
            {
                "id": "external_provider_anthropic_long_soak",
                "kind": "credentialed",
                "required": True,
                "status": "missing",
                "reason": missing_provider_diagnostic["reason"],
                "evidence": ["missing"],
            },
            {
                "id": "manual_tty_30_min_bake",
                "kind": "interactive",
                "required": True,
                "status": "missing",
                "reason": manual_tty_diagnostic["reason"],
                "evidence": ["missing"],
            },
        ],
        [
            by_id["external_provider_anthropic_long_soak"],
            by_id["manual_tty_30_min_bake"],
        ],
    )
    synthetic_actions = {item["id"]: item["next_action"] for item in synthetic_blockers}
    assertions.append(
        ok_assertion(
            "readiness_status_reports_machine_readable_next_actions",
            "release_provider_long_soak.py --write-profile-template"
            in synthetic_actions.get("external_provider_anthropic_long_soak", "")
            and "release_manual_tty_bake.py --record"
            in synthetic_actions.get("manual_tty_30_min_bake", ""),
            actions=synthetic_actions,
        )
    )

    all_ok = all(item["ok"] for item in assertions)
    stdout = json.dumps(
        {
            "readiness_contract": str(READINESS_PATH),
            "required_gates": sorted(EXPECTED_REQUIRED_GATES),
            "blocking_manual_or_credentialed": sorted(
                gate.get("id") for gate in manual_or_credentialed
            ),
            "ok": all_ok,
        },
        indent=2,
        ensure_ascii=False,
    )
    write_command_log(ctx, ["python3", str(Path(__file__).relative_to(ROOT))], stdout, "", 0 if all_ok else 1)
    write_assertions(ctx, status="passed" if all_ok else "failed", assertions=assertions)
    print(stdout)
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
