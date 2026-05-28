#!/usr/bin/env python3
"""Audit one-hour coding sprint evidence for release readiness."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from collections import defaultdict
from typing import Any

DEFAULT_ROOT = Path("/tmp/mossen-release-readiness/coding-sprint")

REQUIRED_CHECKS = (
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
)

TRANSCRIPT_SIGNAL_PATTERNS = {
    "background_agent_used": (r"\bAgent\b", r"async_launched", r"background agent"),
    "taskoutput_retrieved": (r"\bTaskOutput\b", r"task_output"),
    "file_edit_performed": (r"\bEdit\b", r"\bWrite\b", r"file_edit", r"apply_patch"),
    "bash_command_run": (r"\bBash\b", r"command started", r'"command"'),
    "interrupt_or_resume_exercised": (
        r"Ctrl\+C",
        r"\binterrupt",
        r"\bcancel",
        r"\bresume",
        r"--resume",
        r"/resume",
    ),
    "final_summary_recorded": (r"Final Summary", r"final_summary"),
}

ACTUAL_TOOL_SIGNALS = {
    "background_agent_used": {"Agent"},
    "taskoutput_retrieved": {"TaskOutput"},
    "file_edit_performed": {"Edit", "Write"},
    "bash_command_run": {"Bash"},
}


def load_json(path: Path) -> dict[str, Any] | None:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return payload if isinstance(payload, dict) else None


def nonempty_string(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def resolve_summary_relative_path(summary_path: Path, value: Any) -> Path | None:
    if not nonempty_string(value):
        return None
    path = Path(str(value))
    if path.is_absolute():
        return path
    return summary_path.parent / path


def input_responsiveness_evidence_status(
    summary: dict[str, Any],
    summary_path: Path,
) -> dict[str, Any]:
    evidence_path = resolve_summary_relative_path(
        summary_path,
        summary.get("input_responsiveness_evidence"),
    )
    if evidence_path is None:
        return {"ok": False, "reason": "missing input_responsiveness_evidence path"}
    payload = load_json(evidence_path)
    if payload is None:
        return {"ok": False, "reason": "evidence JSON is missing or unreadable", "path": str(evidence_path)}
    observations = payload.get("observations")
    observations_ok = isinstance(observations, list) and any(
        isinstance(item, dict)
        and nonempty_string(item.get("action"))
        and nonempty_string(item.get("result"))
        for item in observations
    )
    ok = (
        payload.get("ok") is True
        and payload.get("during_background_agent_work") is True
        and nonempty_string(payload.get("method"))
        and nonempty_string(payload.get("observed_at"))
        and observations_ok
    )
    return {
        "ok": ok,
        "path": str(evidence_path),
        "method": payload.get("method"),
        "observed_at": payload.get("observed_at"),
        "during_background_agent_work": payload.get("during_background_agent_work"),
        "observations_count": len(observations) if isinstance(observations, list) else 0,
    }


def transcript_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def embedded_exit_code(value: Any) -> int | None:
    if isinstance(value, dict):
        raw = value.get("exit_code")
        if isinstance(raw, int):
            return raw
        if isinstance(raw, float):
            return int(raw)
        return None
    if not isinstance(value, str) or not value:
        return None
    try:
        parsed = json.loads(value)
    except json.JSONDecodeError:
        parsed = None
    parsed_code = embedded_exit_code(parsed)
    if parsed_code is not None:
        return parsed_code
    match = re.search(r'"exit_code"\s*:\s*(-?\d+)', value)
    if match:
        return int(match.group(1))
    match = re.search(r"\bexit\s+(-?\d+)\b", value)
    if match:
        return int(match.group(1))
    return None


def tool_bash_exit_code(payload: dict[str, Any]) -> int | None:
    if payload.get("type") == "tool_use_summary" and payload.get("tool_name") == "Bash":
        for key in ("full_content", "summary"):
            code = embedded_exit_code(payload.get(key))
            if code is not None:
                return code
    if payload.get("type") == "render_event" and payload.get("kind") == "command_finished":
        event_payload = payload.get("payload")
        if isinstance(event_payload, dict):
            raw = event_payload.get("exitCode")
            if isinstance(raw, int):
                return raw
            if isinstance(raw, float):
                return int(raw)
    return None


def bash_command_by_tool_use_id(text: str) -> dict[str, str]:
    commands: dict[str, str] = {}
    for line in text.splitlines():
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(payload, dict) or payload.get("type") != "assistant":
            continue
        message = payload.get("message")
        if not isinstance(message, dict):
            continue
        content = message.get("content")
        if not isinstance(content, list):
            continue
        for item in content:
            if not isinstance(item, dict) or item.get("name") != "Bash":
                continue
            tool_use_id = item.get("id")
            tool_input = item.get("input")
            command = tool_input.get("command") if isinstance(tool_input, dict) else None
            if isinstance(tool_use_id, str) and isinstance(command, str) and command.strip():
                commands[tool_use_id] = command
    return commands


def tool_bash_validation_run(
    payload: dict[str, Any],
    command_by_tool_use_id: dict[str, str] | None = None,
) -> bool:
    if payload.get("type") != "tool_use_summary" or payload.get("tool_name") != "Bash":
        return False
    text_parts = [str(payload.get(key) or "") for key in ("full_content", "summary")]
    if command_by_tool_use_id:
        tool_use_id = payload.get("tool_use_id")
        if isinstance(tool_use_id, str):
            text_parts.append(command_by_tool_use_id.get(tool_use_id, ""))
    text = "\n".join(text_parts).lower()
    validation_markers = (
        "test session starts",
        "pytest",
        "cargo test",
        "cargo check",
        "cargo clippy",
        "npm test",
        "pnpm test",
        "yarn test",
    )
    return any(marker in text for marker in validation_markers)


def validation_output_failed(payload: dict[str, Any]) -> bool:
    text = "\n".join(
        str(payload.get(key) or "")
        for key in ("full_content", "summary")
    )
    failure_patterns = (
        r"(?m)^FAILED\s+",
        r"=+\s+FAILURES\s+=+",
        r"\b[1-9]\d*\s+failed\b",
        r"\b[1-9]\d*\s+error(?:s)?\b",
        r"\bAssertionError\b",
    )
    return any(re.search(pattern, text, re.IGNORECASE) for pattern in failure_patterns)


def transcript_task_statuses(text: str) -> dict[str, str]:
    statuses: dict[str, str] = {}
    for line in text.splitlines():
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(payload, dict) or payload.get("type") != "runner_task_finish":
            continue
        task_id = payload.get("task_id")
        status = payload.get("status")
        if isinstance(task_id, str) and isinstance(status, str):
            statuses[task_id] = status
        elif isinstance(task_id, str):
            exit_code = payload.get("exit_code")
            statuses[task_id] = "completed" if exit_code == 0 else "failed"
    return statuses


def transcript_task_validation_report(text: str) -> dict[str, dict[str, Any]]:
    report: dict[str, dict[str, Any]] = {}
    command_by_tool_use_id = bash_command_by_tool_use_id(text)
    for line in text.splitlines():
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(payload, dict):
            continue
        task_id = payload.get("runner_task_id")
        if not isinstance(task_id, str) or not task_id:
            continue
        if not tool_bash_validation_run(payload, command_by_tool_use_id):
            continue
        exit_code = tool_bash_exit_code(payload)
        if exit_code is None:
            continue
        item = report.setdefault(task_id, {"validation_count": 0})
        item["validation_count"] += 1
        item["last_validation_exit_code"] = exit_code
        item["last_validation_output_failed"] = validation_output_failed(payload)
        item["last_validation_passed"] = exit_code == 0 and not item["last_validation_output_failed"]
        tool_use_id = payload.get("tool_use_id")
        if isinstance(tool_use_id, str) and tool_use_id in command_by_tool_use_id:
            item["last_validation_command"] = command_by_tool_use_id[tool_use_id]
    return report


def transcript_task_tools(text: str) -> dict[str, set[str]]:
    tools: dict[str, set[str]] = defaultdict(set)
    for line in text.splitlines():
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(payload, dict) or payload.get("type") != "tool_use_summary":
            continue
        task_id = payload.get("runner_task_id")
        tool_name = payload.get("tool_name")
        if isinstance(task_id, str) and isinstance(tool_name, str):
            tools[task_id].add(tool_name)
    return dict(tools)


def task_changed_files(tools: set[str]) -> bool:
    return bool(tools & {"Edit", "Write"})


def assistant_message_text(payload: dict[str, Any]) -> str:
    message = payload.get("message")
    if isinstance(message, str):
        return message
    if not isinstance(message, dict):
        return ""
    content = message.get("content")
    if isinstance(content, str):
        return content
    if not isinstance(content, list):
        return ""
    parts: list[str] = []
    for item in content:
        if isinstance(item, str):
            parts.append(item)
        elif isinstance(item, dict) and isinstance(item.get("text"), str):
            parts.append(item["text"])
    return "\n".join(parts)


def summary_claims_file_changes(text: str) -> bool:
    return bool(
        re.search(
            r"(?im)^\s*(?:\*\*)?("
            r"changed files?|files changed|new files?|modified files?|"
            r"deleted files?|removed files?"
            r")(?:\*\*)?\s*:",
            text,
        )
    )


def task_summary_activity_mismatches(text: str) -> list[dict[str, Any]]:
    tools_by_task = transcript_task_tools(text)
    mismatches: list[dict[str, Any]] = []
    for line in text.splitlines():
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(payload, dict) or payload.get("type") != "assistant":
            continue
        task_id = payload.get("runner_task_id")
        if not isinstance(task_id, str) or not task_id:
            continue
        message = assistant_message_text(payload)
        if "Final Summary" not in message or not summary_claims_file_changes(message):
            continue
        tools = tools_by_task.get(task_id, set())
        if task_changed_files(tools):
            continue
        mismatches.append(
            {
                "id": task_id,
                "reason": "final summary claims file changes but no Edit/Write tool ran",
                "tools": sorted(tools),
            }
        )
    return mismatches


def transcript_validation_failures(text: str) -> list[dict[str, Any]]:
    statuses = transcript_task_statuses(text)
    report = transcript_task_validation_report(text)
    tools_by_task = transcript_task_tools(text)
    failures: list[dict[str, Any]] = []
    prior_validation_passed = False
    for task_id, status in statuses.items():
        if status != "completed":
            continue
        item = report.get(task_id, {})
        if item.get("last_validation_passed") is True:
            prior_validation_passed = True
            continue
        changed = task_changed_files(tools_by_task.get(task_id, set()))
        if not changed and prior_validation_passed:
            continue
        prior_validation_passed = False
        failures.append(
            {
                "id": task_id,
                "status": status,
                "changed_files": changed,
                "validation_count": item.get("validation_count", 0),
                "last_validation_exit_code": item.get("last_validation_exit_code"),
                "last_validation_output_failed": item.get("last_validation_output_failed"),
            }
        )
    return failures


def transcript_signals(text: str) -> dict[str, bool]:
    actual_tools: set[str] = set()
    non_prompt_text: list[str] = []
    resume_seen = False
    turn_limit_seen = False

    for line in text.splitlines():
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(payload, dict):
            continue

        record_type = payload.get("type")
        if record_type == "tool_use_summary":
            tool_name = payload.get("tool_name")
            if isinstance(tool_name, str) and tool_name:
                actual_tools.add(tool_name)
        elif record_type == "runner_task_start":
            command = payload.get("command")
            if payload.get("resume_note") or (
                isinstance(command, list) and "--restore-id" in command
            ):
                resume_seen = True
        elif record_type == "result":
            terminal = payload.get("terminal")
            if isinstance(terminal, str) and "MaxTurns" in terminal:
                turn_limit_seen = True

        # Exclude runner plans, user prompts, and system init tool inventories;
        # those mention required tools without proving they actually ran.
        if record_type in {
            "assistant",
            "result",
            "runner_text",
            "runner_task_finish",
            "runner_finish",
        }:
            non_prompt_text.append(json.dumps(payload, ensure_ascii=False))

    body = "\n".join(non_prompt_text)
    signals: dict[str, bool] = {}
    for name, tools in ACTUAL_TOOL_SIGNALS.items():
        signals[name] = bool(actual_tools & tools)
    signals["interrupt_or_resume_exercised"] = resume_seen or any(
        re.search(pattern, body, re.IGNORECASE)
        for pattern in TRANSCRIPT_SIGNAL_PATTERNS["interrupt_or_resume_exercised"]
    )
    signals["final_summary_recorded"] = any(
        re.search(pattern, body, re.IGNORECASE)
        for pattern in TRANSCRIPT_SIGNAL_PATTERNS["final_summary_recorded"]
    )
    signals["tasks_finished_without_turn_limit"] = not turn_limit_seen
    signals["per_task_validation_bash_succeeded"] = not transcript_validation_failures(text)
    signals["final_summary_matches_activity"] = not task_summary_activity_mismatches(text)
    return signals


def completed_tasks(summary: dict[str, Any]) -> int:
    tasks = summary.get("tasks")
    if isinstance(tasks, list):
        count = 0
        for task in tasks:
            if not isinstance(task, dict):
                continue
            status = str(task.get("status") or task.get("outcome") or "").lower()
            if status in {"done", "pass", "passed", "complete", "completed"}:
                count += 1
        return count
    value = summary.get("tasks_completed")
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        return int(value)
    return 0


def task_limit_failures(summary: dict[str, Any]) -> list[dict[str, Any]]:
    tasks = summary.get("tasks")
    if not isinstance(tasks, list):
        return []
    failures: list[dict[str, Any]] = []
    for task in tasks:
        if not isinstance(task, dict):
            continue
        terminal = str(task.get("terminal") or "")
        status = str(task.get("status") or "").lower()
        if "MaxTurns" in terminal or "turn_limit" in status:
            failures.append(
                {
                    "id": task.get("id"),
                    "status": task.get("status"),
                    "terminal": task.get("terminal"),
                }
            )
    return failures


def summary_check(summary: dict[str, Any], name: str) -> bool:
    checks = summary.get("checks")
    if isinstance(checks, dict) and checks.get(name) is True:
        return True
    return summary.get(name) is True


def ok_assertion(name: str, ok: bool, **detail: Any) -> dict[str, Any]:
    return {"name": name, "ok": ok, **detail}


def audit(
    *,
    summary_path: Path,
    transcript_path: Path,
    minimum_minutes: float,
    minimum_tasks: int,
) -> dict[str, Any]:
    summary = load_json(summary_path)
    text = transcript_text(transcript_path)
    signals = transcript_signals(text)
    assertions: list[dict[str, Any]] = []

    assertions.append(
        ok_assertion(
            "summary_json_readable",
            summary is not None,
            summary_path=str(summary_path),
        )
    )
    assertions.append(
        ok_assertion(
            "transcript_jsonl_present",
            transcript_path.exists() and bool(text.strip()),
            transcript_path=str(transcript_path),
            bytes=len(text.encode("utf-8")),
        )
    )

    if summary is None:
        return {
            "ok": False,
            "summary_path": str(summary_path),
            "transcript_path": str(transcript_path),
            "assertions": assertions,
        }

    duration = float(summary.get("duration_minutes") or 0)
    task_count = completed_tasks(summary)
    limit_failures = task_limit_failures(summary)
    validation_failures = transcript_validation_failures(text)
    summary_mismatches = task_summary_activity_mismatches(text)
    input_evidence = input_responsiveness_evidence_status(summary, summary_path)
    assertions.extend(
        [
            ok_assertion("summary_ok_true", summary.get("ok") is True, ok_value=summary.get("ok")),
            ok_assertion(
                "duration_meets_floor",
                duration >= minimum_minutes,
                duration_minutes=duration,
                minimum_minutes=minimum_minutes,
            ),
            ok_assertion(
                "completed_nontrivial_task_count_meets_floor",
                task_count >= minimum_tasks,
                tasks_completed=task_count,
                minimum_tasks=minimum_tasks,
            ),
            ok_assertion(
                "no_task_stopped_by_turn_limit",
                not limit_failures,
                failures=limit_failures,
            ),
            ok_assertion(
                "per_task_last_bash_validation_passed",
                not validation_failures,
                failures=validation_failures,
            ),
            ok_assertion(
                "input_responsiveness_evidence_valid",
                input_evidence["ok"],
                evidence=input_evidence,
            ),
            ok_assertion(
                "final_summaries_match_task_activity",
                not summary_mismatches,
                failures=summary_mismatches,
            ),
        ]
    )

    for check in REQUIRED_CHECKS:
        assertions.append(
            ok_assertion(
                f"summary_check_{check}",
                summary_check(summary, check),
                check=check,
            )
        )

    for signal, seen in signals.items():
        assertions.append(
            ok_assertion(
                f"transcript_signal_{signal}",
                seen,
                signal=signal,
            )
        )

    all_ok = all(item["ok"] for item in assertions)
    return {
        "ok": all_ok,
        "summary_path": str(summary_path),
        "transcript_path": str(transcript_path),
        "duration_minutes": duration,
        "tasks_completed": task_count,
        "transcript_signals": signals,
        "summary_activity_mismatches": summary_mismatches,
        "required_checks": list(REQUIRED_CHECKS),
        "assertions": assertions,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--summary", type=Path, default=DEFAULT_ROOT / "summary.json")
    parser.add_argument("--transcript", type=Path, default=DEFAULT_ROOT / "transcript.jsonl")
    parser.add_argument("--minimum-minutes", type=float, default=60)
    parser.add_argument("--minimum-tasks", type=int, default=5)
    args = parser.parse_args()

    report = audit(
        summary_path=args.summary,
        transcript_path=args.transcript,
        minimum_minutes=args.minimum_minutes,
        minimum_tasks=args.minimum_tasks,
    )
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
