#!/usr/bin/env python3
"""Evaluate current production release readiness evidence."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
READINESS_PATH = ROOT / "docs" / "release_readiness.v1.json"
DEFAULT_OUTPUT_DIR = Path("/tmp/mossen-release-readiness")
DEFAULT_PROFILE_CONFIG = Path.home() / ".mossen" / "settings.json"

REQUIRED_PACKAGE_ASSERTIONS = {
    "cargo_install_release_exits_zero",
    "installed_binary_exists_and_is_executable",
    "installed_binary_is_resolved_from_path",
    "installed_binary_runs_outside_repo_cwd",
    "first_launch_profile_query_exits_zero",
    "startup_latency_with_installed_binary_is_bounded",
    "fallback_profile_migration_exits_zero",
    "config_migration_writes_isolated_settings",
    "post_migration_profile_visible_without_secrets",
    "artifact_env_redacts_sensitive_values",
}

REQUIRED_MANUAL_TTY_CHECKS = {
    "input_responsive",
    "selection_copy_works",
    "scroll_returns_to_bottom",
    "subagent_completion_feedback",
    "ctrl_c_safe",
    "no_panic_or_deadlock",
    "no_render_tearing",
}

_RUST_SOURCE_LATEST_MTIME: float | None = None


def load_json(path: Path) -> dict[str, Any] | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def profile_config_path() -> Path:
    override = os.environ.get("MOSSEN_RELEASE_PROFILE_CONFIG")
    return Path(override) if override else DEFAULT_PROFILE_CONFIG


def profile_value(profile: dict[str, Any], *keys: str) -> str | None:
    for key in keys:
        value = profile.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return None


def configured_profiles(path: Path) -> dict[str, dict[str, Any]]:
    payload = load_json(path)
    if payload is None:
        return {}
    raw_profiles = payload.get("mossen.profiles") or payload.get("profiles") or {}
    if not isinstance(raw_profiles, dict):
        return {}
    profiles: dict[str, dict[str, Any]] = {}
    for name, value in raw_profiles.items():
        if isinstance(name, str) and isinstance(value, dict):
            profiles[name] = value
    return profiles


def provider_profile_hint(protocol: str | None) -> str:
    if not protocol:
        return "gate does not declare required provider protocol"
    config_path = profile_config_path()
    profiles = configured_profiles(config_path)
    matching = sorted(
        name
        for name, profile in profiles.items()
        if profile_value(profile, "provider") == protocol
    )
    if matching:
        return f"configured {protocol} profile(s): {', '.join(matching)}"
    available: dict[str, list[str]] = {}
    for name, profile in sorted(profiles.items()):
        provider = profile_value(profile, "provider") or "<missing-provider>"
        available.setdefault(provider, []).append(name)
    available_text = (
        ", ".join(f"{provider}: {','.join(names)}" for provider, names in sorted(available.items()))
        if available
        else "<none>"
    )
    return (
        f"no configured {protocol} profile in {config_path}; "
        f"available provider profiles: {available_text}; "
        "generate template: python3 scripts/release_provider_long_soak.py --write-profile-template"
    )


def resolve_path(path: str) -> Path:
    candidate = Path(path)
    if candidate.is_absolute():
        return candidate
    return ROOT / candidate


def rust_source_files() -> list[Path]:
    files = [path for path in (ROOT / "Cargo.toml", ROOT / "Cargo.lock") if path.exists()]
    crates = ROOT / "crates"
    if crates.exists():
        for path in crates.rglob("*"):
            if path.is_file() and (
                path.suffix == ".rs" or path.name in {"Cargo.toml", "build.rs"}
            ):
                files.append(path)
    return files


def rust_source_latest_mtime() -> float:
    global _RUST_SOURCE_LATEST_MTIME
    if _RUST_SOURCE_LATEST_MTIME is not None:
        return _RUST_SOURCE_LATEST_MTIME
    mtimes = [path.stat().st_mtime for path in rust_source_files()]
    _RUST_SOURCE_LATEST_MTIME = max(mtimes) if mtimes else 0.0
    return _RUST_SOURCE_LATEST_MTIME


def evidence_freshness_status(
    evidence_paths: list[Path],
    latest_source_mtime: float,
    source_label: str,
) -> tuple[bool, str]:
    missing = [str(path) for path in evidence_paths if not path.exists()]
    if missing:
        return False, f"missing evidence files: {missing}"
    stale = [
        str(path)
        for path in evidence_paths
        if path.stat().st_mtime + 0.001 < latest_source_mtime
    ]
    if stale:
        return False, f"stale evidence older than {source_label}: {stale}"
    return True, f"evidence is current for {source_label}"


def latest_mtime(paths: list[Path]) -> float:
    return max((path.stat().st_mtime for path in paths if path.exists()), default=0.0)


def script_path(name: str) -> Path:
    return ROOT / "scripts" / name


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def harness_evidence_source_files() -> list[Path]:
    files = [
        ROOT / "docs" / "capability_matrix.v1.json",
        script_path("harness_assertions_aggregator.py"),
        script_path("harness_capability_matrix.py"),
    ]
    scripts = ROOT / "scripts"
    if scripts.exists():
        for pattern in ("harness_[MR]*.py", "wave_w*.py"):
            files.extend(path for path in scripts.glob(pattern) if path.is_file())
    return files


def freshness_baselines() -> dict[str, Any]:
    rust_files = rust_source_files()
    harness_files = harness_evidence_source_files()
    latest_rust = max(rust_files, key=lambda path: path.stat().st_mtime) if rust_files else None
    latest_harness = (
        max(harness_files, key=lambda path: path.stat().st_mtime) if harness_files else None
    )
    return {
        "rust_source": {
            "file_count": len(rust_files),
            "latest_mtime": rust_source_latest_mtime(),
            "latest_file": str(latest_rust) if latest_rust else None,
        },
        "harness_sources": {
            "file_count": len(harness_files),
            "latest_mtime": latest_mtime(harness_files),
            "latest_file": str(latest_harness) if latest_harness else None,
        },
    }


def assertion_item_passed(item: dict[str, Any]) -> bool:
    if "ok" in item:
        return item.get("ok") is True
    if "passed" in item:
        return item.get("passed") is True
    return False


def assertions_file_passed(path: Path) -> tuple[bool, str]:
    payload = load_json(path)
    if payload is None:
        return False, "assertions JSON missing or unreadable"
    if payload.get("status") != "passed":
        return False, f"assertions status is {payload.get('status')!r}"
    failed = [
        item.get("name", "<unnamed>")
        for item in payload.get("assertions", [])
        if isinstance(item, dict) and not assertion_item_passed(item)
    ]
    if failed:
        return False, f"failed assertions: {failed[:5]}"
    return True, "assertions passed"


def deterministic_harness_matrix_status() -> tuple[str, str]:
    final_path = ROOT / "harness-final-report.json"
    capability_path = ROOT / "harness-capability-report.json"
    final = load_json(final_path)
    capability = load_json(capability_path)
    if final is None or capability is None:
        return "missing", "harness final or capability report is missing"
    non_passed = {
        key: value
        for key, value in (final.get("by_status_counts") or {}).items()
        if key != "passed" and value
    }
    summary = capability.get("summary_by_status") or {}
    bad_caps = {
        key: summary.get(key, 0)
        for key in ("fail", "missing", "stale", "partial")
        if summary.get(key, 0)
    }
    coverage = capability.get("script_coverage") or {}
    unmapped = coverage.get("unmapped_mr_scripts") or []
    multi = coverage.get("multi_mapped_mr_scripts") or {}
    if non_passed or bad_caps or unmapped or multi:
        return (
            "failed",
            f"non_passed={non_passed}, bad_caps={bad_caps}, unmapped={len(unmapped)}, multi={len(multi)}",
        )
    fresh, reason = evidence_freshness_status(
        [final_path, capability_path],
        latest_mtime(harness_evidence_source_files()),
        "harness scripts and capability matrix",
    )
    if not fresh:
        return "stale", reason
    return (
        "passed",
        f"{final.get('total_tests')} harness tests, {capability.get('total_capabilities')} capabilities",
    )


def cargo_workspace_status(log_path: Path) -> tuple[str, str]:
    if not log_path.exists():
        return "missing", f"missing log: {log_path}"
    text = log_path.read_text(encoding="utf-8", errors="replace")
    if "test result: FAILED" in text or "error:" in text:
        return "failed", "workspace cargo test log contains failure markers"
    if "test result: ok" not in text:
        return "incomplete", "workspace cargo test log lacks ok summaries"
    fresh, reason = evidence_freshness_status(
        [log_path], rust_source_latest_mtime(), "Rust source tree"
    )
    if not fresh:
        return "stale", reason
    return "passed", f"workspace test log present and current: {log_path}"


def warnings_as_errors_status() -> tuple[str, str]:
    json_path = Path("/tmp/mossen-release-readiness/warnings-as-errors.json")
    log_path = Path("/tmp/mossen-release-readiness/warnings-as-errors.log")
    payload = load_json(json_path)
    if payload is not None:
        if payload.get("exit_code") == 0:
            evidence_paths = [json_path]
            if log_path.exists():
                evidence_paths.append(log_path)
            fresh, reason = evidence_freshness_status(
                evidence_paths, rust_source_latest_mtime(), "Rust source tree"
            )
            if not fresh:
                return "stale", reason
            return "passed", f"warnings-as-errors passed and current: {json_path}"
        return "failed", f"warnings-as-errors exit_code={payload.get('exit_code')}"
    if log_path.exists():
        text = log_path.read_text(encoding="utf-8", errors="replace")
        if "Finished `" in text and "warning:" not in text and "error:" not in text:
            fresh, reason = evidence_freshness_status(
                [log_path], rust_source_latest_mtime(), "Rust source tree"
            )
            if not fresh:
                return "stale", reason
            return "passed", f"warnings-as-errors log passed and current: {log_path}"
        return "failed", "warnings-as-errors log contains warning/error or lacks success marker"
    return "missing", "missing warnings-as-errors evidence"


def package_install_status(path: Path) -> tuple[str, str]:
    ok, reason = assertions_file_passed(path)
    if not ok:
        return "failed", reason
    payload = load_json(path) or {}
    names = {
        item.get("name")
        for item in payload.get("assertions", [])
        if isinstance(item, dict)
    }
    missing = sorted(REQUIRED_PACKAGE_ASSERTIONS - names)
    if missing:
        return "incomplete", f"package install evidence missing assertions: {missing}"
    latest = max(
        rust_source_latest_mtime(),
        latest_mtime([script_path("harness_R11_2_package_install_smoke.py")]),
    )
    fresh, fresh_reason = evidence_freshness_status(
        [path], latest, "Rust source tree and package install harness"
    )
    if not fresh:
        return "stale", fresh_reason
    return "passed", "assertions passed with current production install coverage"


def provider_soak_status(gate: dict[str, Any]) -> tuple[str, str]:
    evidence = gate.get("evidence") or []
    if not evidence:
        return "missing", "missing soak evidence path in contract"
    report_path = resolve_path(evidence[0])
    report = load_json(report_path)
    if report is None:
        command = gate.get("command") or "<missing command>"
        hint = provider_profile_hint(gate.get("protocol"))
        return "missing", f"missing soak report: {report_path}; {hint}; run: {command}"
    minimum = float(gate.get("minimum_duration_minutes", 0)) * 60
    actual = float(report.get("wall_duration_secs") or 0)
    if report.get("ok") is not True:
        return "failed", f"soak report ok={report.get('ok')} failed_attempts={report.get('failed_attempts')}"
    if actual < minimum:
        return "incomplete", f"duration {actual:.1f}s is below required {minimum:.1f}s"
    required_scenarios = set(gate.get("required_scenarios") or [])
    completed_scenarios = set(report.get("scenarios_completed") or [])
    missing_scenarios = sorted(required_scenarios - completed_scenarios)
    if missing_scenarios:
        return "incomplete", f"missing provider scenarios: {missing_scenarios}"
    fresh, fresh_reason = evidence_freshness_status(
        [report_path], rust_source_latest_mtime(), "Rust source tree"
    )
    if not fresh:
        return "stale", fresh_reason
    return "passed", f"{report.get('started_attempts')} attempts over {actual:.1f}s"


def pty_soak_status(gate: dict[str, Any]) -> tuple[str, str]:
    evidence = gate.get("evidence") or []
    if not evidence:
        return "missing", "missing PTY evidence path in contract"
    path = resolve_path(evidence[0])
    ok, reason = assertions_file_passed(path)
    if not ok:
        return "failed" if path.exists() else "missing", reason
    payload = load_json(path) or {}
    required = float(gate.get("minimum_duration_minutes", 0)) * 60
    elapsed = 0.0
    for item in payload.get("assertions", []):
        if item.get("name") != "stream_duration_floor":
            continue
        match = re.search(r"stream_elapsed=([0-9.]+)s", str(item.get("evidence", "")))
        if match:
            elapsed = float(match.group(1))
    if elapsed < required:
        return "incomplete", f"PTY stream duration {elapsed:.1f}s is below required {required:.1f}s"
    latest = max(
        rust_source_latest_mtime(),
        latest_mtime([script_path("wave_w107_render_pty_long_matrix_soak.py")]),
    )
    fresh, fresh_reason = evidence_freshness_status(
        [path], latest, "Rust source tree and PTY soak harness"
    )
    if not fresh:
        return "stale", fresh_reason
    return "passed", f"PTY stream duration {elapsed:.1f}s"


def manual_tty_bake_status(gate: dict[str, Any]) -> tuple[str, str]:
    evidence = gate.get("evidence") or []
    if not evidence:
        return "missing", "missing manual TTY evidence path in contract"
    path = resolve_path(evidence[0])
    payload = load_json(path)
    if payload is None:
        command = gate.get("command") or "python3 scripts/release_manual_tty_bake.py --record"
        return "missing", f"missing manual TTY evidence: {path}; start with: {command}"
    checks = payload.get("checks")
    if (
        payload.get("ok") is False
        and not str(payload.get("started_at") or "").strip()
        and not str(payload.get("ended_at") or "").strip()
        and isinstance(checks, dict)
        and checks
        and all(value is False for value in checks.values())
    ):
        command = gate.get("command") or "python3 scripts/release_manual_tty_bake.py --record"
        return (
            "missing",
            f"manual TTY evidence appears to be an unfilled template: {path}; record real evidence with: {command}",
        )
    if payload.get("ok") is not True:
        return "failed", f"manual TTY evidence ok={payload.get('ok')!r}"

    minimum = float(gate.get("minimum_duration_minutes", 0))
    duration = float(payload.get("duration_minutes") or 0)
    if duration < minimum:
        return "incomplete", f"manual TTY duration {duration:.1f}m is below required {minimum:.1f}m"

    if not isinstance(checks, dict):
        return "failed", "manual TTY evidence must include checks object"
    required_checks = set(gate.get("required_checks") or REQUIRED_MANUAL_TTY_CHECKS)
    missing_checks = sorted(required_checks - set(checks))
    failed_checks = sorted(check for check in required_checks if checks.get(check) is not True)
    if missing_checks:
        return "incomplete", f"manual TTY evidence missing checks: {missing_checks}"
    if failed_checks:
        return "failed", f"manual TTY checks failed: {failed_checks}"

    terminal = str(payload.get("terminal_app") or "").strip()
    command = str(payload.get("mossen_command") or "").strip()
    if not terminal or not command:
        return "incomplete", "manual TTY evidence must record terminal_app and mossen_command"
    return "passed", f"manual TTY bake duration {duration:.1f}m in {terminal}"


def coding_sprint_status(gate: dict[str, Any]) -> tuple[str, str]:
    evidence = [resolve_path(path) for path in gate.get("evidence", [])]
    missing = [str(path) for path in evidence if not path.exists()]
    if missing:
        return "missing", f"missing coding sprint evidence: {missing}"
    by_name = {path.name: path for path in evidence}
    transcript = by_name.get("transcript.jsonl")
    summary = by_name.get("summary.json")
    if transcript is None or summary is None:
        return "failed", "coding sprint evidence must include transcript.jsonl and summary.json"
    audit = subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts" / "release_coding_sprint_audit.py"),
            "--transcript",
            str(transcript),
            "--summary",
            str(summary),
            "--minimum-minutes",
            str(gate.get("minimum_duration_minutes", 0)),
            "--minimum-tasks",
            str(gate.get("minimum_tasks", 5)),
        ],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        check=False,
    )
    payload = None
    try:
        payload = json.loads(audit.stdout)
    except json.JSONDecodeError:
        pass
    if audit.returncode != 0:
        failed = []
        if isinstance(payload, dict):
            failed = [
                item.get("name", "<unnamed>")
                for item in payload.get("assertions", [])
                if isinstance(item, dict) and item.get("ok") is not True
            ]
        detail = f": {failed[:5]}" if failed else ""
        return "failed", f"coding sprint audit failed{detail}"
    if not isinstance(payload, dict):
        return "failed", "coding sprint audit did not emit JSON"
    latest = max(
        rust_source_latest_mtime(),
        latest_mtime(
            [
                script_path("release_coding_sprint_runner.py"),
                script_path("release_coding_sprint_audit.py"),
            ]
        ),
    )
    fresh, fresh_reason = evidence_freshness_status(
        evidence, latest, "Rust source tree and coding sprint harness"
    )
    if not fresh:
        return "stale", fresh_reason
    return (
        "passed",
        f"coding sprint duration {float(payload.get('duration_minutes') or 0):.1f}m, "
        f"tasks {payload.get('tasks_completed')}",
    )


def evaluate_gate(gate: dict[str, Any]) -> dict[str, Any]:
    gate_id = gate["id"]
    if gate_id == "deterministic_harness_matrix":
        status, reason = deterministic_harness_matrix_status()
    elif gate_id == "rust_workspace_tests":
        status, reason = cargo_workspace_status(resolve_path(gate["evidence"][0]))
    elif gate_id == "warnings_as_errors":
        status, reason = warnings_as_errors_status()
    elif gate_id == "package_install_smoke":
        status, reason = package_install_status(resolve_path(gate["evidence"][0]))
    elif gate.get("kind") == "credentialed":
        status, reason = provider_soak_status(gate)
    elif gate_id == "pty_30_min_soak":
        status, reason = pty_soak_status(gate)
    elif gate_id == "manual_tty_30_min_bake":
        status, reason = manual_tty_bake_status(gate)
    elif gate_id == "one_hour_coding_sprint":
        status, reason = coding_sprint_status(gate)
    else:
        status, reason = "missing", "no evaluator for gate"
    return {
        "id": gate_id,
        "kind": gate.get("kind"),
        "required": gate.get("required") is True,
        "status": status,
        "reason": reason,
        "evidence": gate.get("evidence", []),
    }


def next_action_for_gate(gate: dict[str, Any]) -> str:
    gate_id = gate["id"]
    if gate_id == "external_provider_anthropic_long_soak":
        return (
            "Generate a profile template with "
            "`python3 scripts/release_provider_long_soak.py --write-profile-template`, "
            "add an anthropic profile with real model/apiKey to ~/.mossen/settings.json, then run "
            "`python3 scripts/release_provider_long_soak.py --profile <anthropic-profile> --minutes 30`."
        )
    if gate_id == "external_provider_openai_responses_long_soak":
        return (
            "Generate a profile template with "
            "`python3 scripts/release_provider_long_soak.py --write-profile-template`, "
            "add an openai-responses profile with real model/apiKey to ~/.mossen/settings.json, then run "
            "`python3 scripts/release_provider_long_soak.py --profile <openai-responses-profile> --minutes 30`."
        )
    if gate_id == "manual_tty_30_min_bake":
        return (
            "Create manual bake evidence with "
            "`python3 scripts/release_manual_tty_bake.py --record` after running the release candidate in a real terminal for at least 30 minutes, then run "
            "`python3 scripts/release_manual_tty_bake.py`."
        )
    evidence = gate.get("evidence") or []
    command = gate.get("command")
    if command:
        return f"Refresh evidence with `{command}`."
    if evidence:
        return f"Provide current evidence at {', '.join(evidence)}."
    return "Inspect the gate contract and provide current passing evidence."


def blocked_gate_rows(gates: list[dict[str, Any]], contract_gates: list[dict[str, Any]]) -> list[dict[str, Any]]:
    contract_by_id = {gate.get("id"): gate for gate in contract_gates}
    rows = []
    for gate in gates:
        if not gate["required"] or gate["status"] == "passed":
            continue
        contract_gate = contract_by_id.get(gate["id"], {})
        rows.append(
            {
                "id": gate["id"],
                "kind": gate["kind"],
                "status": gate["status"],
                "reason": gate["reason"],
                "evidence": gate.get("evidence", []),
                "next_action": next_action_for_gate(contract_gate or gate),
            }
        )
    return rows


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# Release Readiness Status",
        "",
        f"- evaluated at: `{report['evaluated_at']}`",
        f"- ready: `{str(report['ready']).lower()}`",
        f"- required passed: {report['required_passed']}/{report['required_total']}",
        f"- JSON evidence: `{report['output_files']['json']}`",
        f"- Markdown evidence: `{report['output_files']['markdown']}`",
        "",
        "| gate | required | status | reason |",
        "|---|---:|---|---|",
    ]
    for gate in report["gates"]:
        reason = str(gate["reason"]).replace("|", "\\|")
        lines.append(
            f"| `{gate['id']}` | {str(gate['required']).lower()} | `{gate['status']}` | {reason} |"
        )
    if report.get("blocked_gates"):
        lines.extend(["", "## Blocking Next Actions", ""])
        for gate in report["blocked_gates"]:
            lines.append(f"- `{gate['id']}`: {gate['next_action']}")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--readiness", type=Path, default=READINESS_PATH)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR)
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()

    payload = load_json(args.readiness)
    if payload is None:
        raise SystemExit(f"unable to read readiness contract: {args.readiness}")
    contract_gates = payload.get("gates", [])
    gates = [evaluate_gate(gate) for gate in contract_gates]
    required = [gate for gate in gates if gate["required"]]
    required_passed = [gate for gate in required if gate["status"] == "passed"]
    blocked = blocked_gate_rows(gates, contract_gates)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    json_path = args.output_dir / "readiness-status.json"
    md_path = args.output_dir / "readiness-status.md"
    report = {
        "readiness_contract": str(args.readiness),
        "evaluated_at": now_iso(),
        "output_files": {
            "json": str(json_path),
            "markdown": str(md_path),
        },
        "freshness_baselines": freshness_baselines(),
        "ready": len(required_passed) == len(required),
        "required_total": len(required),
        "required_passed": len(required_passed),
        "blocked_gates": blocked,
        "next_actions": [gate["next_action"] for gate in blocked],
        "gates": gates,
    }
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    md_path.write_text(render_markdown(report), encoding="utf-8")
    print(json.dumps(report, indent=2, ensure_ascii=False))
    if args.strict and not report["ready"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
