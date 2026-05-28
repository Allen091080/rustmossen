#!/usr/bin/env python3
"""Run and record a release coding sprint against a real small project."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from release_coding_sprint_audit import (
    input_responsiveness_evidence_status,
    task_summary_activity_mismatches,
    transcript_signals,
    transcript_validation_failures,
)

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ARTIFACT_DIR = Path("/tmp/mossen-release-readiness/coding-sprint")
DEFAULT_PROFILE_CONFIG = Path.home() / ".mossen" / "settings.json"

INSTRUMENTS = "Bash,Read,Write,Edit,Grep,Glob,Agent,TaskOutput"
FINAL_SUMMARY_ACTIVITY_RULE = (
    "Final Summary must match actual tool activity: only list changed/new files "
    "if this task used Edit or Write. If no edit was made, say `No changes made` "
    "and list the Bash validation command only."
)


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def load_json(path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {}
    if isinstance(payload, dict):
        return payload
    return {}


def configured_profiles(path: Path) -> dict[str, dict[str, Any]]:
    payload = load_json(path)
    raw = payload.get("mossen.profiles") or payload.get("profiles") or {}
    if not isinstance(raw, dict):
        return {}
    return {name: value for name, value in raw.items() if isinstance(name, str) and isinstance(value, dict)}


def profile_value(profile: dict[str, Any], *keys: str) -> str | None:
    for key in keys:
        value = profile.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return None


def provider_env(args: argparse.Namespace) -> dict[str, str]:
    profiles = configured_profiles(args.profile_config)
    profile = profiles.get(args.profile)
    if profile is None:
        available = ", ".join(sorted(profiles)) or "<none>"
        raise SystemExit(
            f"profile {args.profile!r} not found in {args.profile_config}; available: {available}"
        )
    protocol = profile_value(profile, "provider")
    base_url = profile_value(profile, "baseURL", "base_url")
    model = profile_value(profile, "model")
    api_key = profile_value(profile, "apiKey", "api_key")
    auth_token = profile_value(profile, "authToken", "auth_token")
    missing = [
        name
        for name, value in [
            ("provider", protocol),
            ("baseURL", base_url),
            ("model", model),
        ]
        if not value
    ]
    if not api_key and not auth_token:
        missing.append("apiKey or authToken")
    if missing:
        raise SystemExit(f"profile {args.profile!r} is missing: {', '.join(missing)}")

    env = os.environ.copy()
    env.update(
        {
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": protocol or "",
            "MOSSEN_CODE_CUSTOM_BASE_URL": base_url or "",
            "MOSSEN_CODE_CUSTOM_MODEL": model or "",
            "MOSSEN_CODE_CUSTOM_NAME": f"release-coding-sprint-{args.profile}",
            "MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS": str(args.request_timeout_secs),
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": str(args.stream_timeout_secs),
            "MOSSEN_CODE_DISABLE_THINKING": "1",
            "MOSSEN_CODE_DISABLE_ADAPTIVE_THINKING": "1",
            "MOSSEN_START_BUILD": "never",
        }
    )
    if api_key:
        env["MOSSEN_CODE_CUSTOM_API_KEY"] = api_key
        env.pop("MOSSEN_CODE_CUSTOM_AUTH_TOKEN", None)
    if auth_token:
        env["MOSSEN_CODE_CUSTOM_AUTH_TOKEN"] = auth_token
        env.pop("MOSSEN_CODE_CUSTOM_API_KEY", None)
    return env


def write_file(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def init_project(work_dir: Path) -> None:
    if work_dir.exists():
        shutil.rmtree(work_dir)
    (work_dir / "sprintcalc").mkdir(parents=True)
    (work_dir / "tests").mkdir(parents=True)
    write_file(
        work_dir / "pyproject.toml",
        """[project]
name = "sprintcalc"
version = "0.1.0"
requires-python = ">=3.10"

[tool.pytest.ini_options]
testpaths = ["tests"]
""",
    )
    write_file(
        work_dir / "README.md",
        """# sprintcalc

A tiny calculator project used by the Mossen release coding sprint.
""",
    )
    write_file(work_dir / "sprintcalc" / "__init__.py", """from .core import evaluate\n""")
    write_file(
        work_dir / "sprintcalc" / "core.py",
        '''"""Core calculator logic."""


def evaluate(expression: str) -> float:
    """Evaluate a simple addition expression.

    The starting implementation is intentionally small; the release sprint
    should evolve it through real code edits and tests.
    """
    total = 0.0
    for part in expression.split("+"):
        total += float(part.strip())
    return total
''',
    )
    write_file(
        work_dir / "tests" / "test_core.py",
        """from sprintcalc import evaluate


def test_addition():
    assert evaluate("1 + 2 + 3") == 6
""",
    )


def task_prompts(min_tasks: int) -> list[dict[str, str]]:
    base = [
        {
            "id": "parser-precedence",
            "prompt": (
                "Task 1/5: Improve this Python package. Implement expression parsing in "
                "sprintcalc/core.py for +, -, *, /, parentheses, whitespace, and unary minus. "
                "Add focused pytest coverage. Use Bash to run tests. Finish with a concise "
                f"Final Summary naming changed files and commands. {FINAL_SUMMARY_ACTIVITY_RULE}"
            ),
        },
        {
            "id": "json-history",
            "prompt": (
                "Task 2/5: Continue the same project. Add sprintcalc/history.py with a small "
                "JSON-backed calculation history API, including append/list/clear behavior and "
                f"pytest coverage. Use Bash to run tests. Finish with a Final Summary. {FINAL_SUMMARY_ACTIVITY_RULE}"
            ),
        },
        {
            "id": "cli-entrypoint",
            "prompt": (
                "Task 3/5: Continue the same project. Add a CLI entrypoint in sprintcalc/cli.py "
                "and sprintcalc/__main__.py so `python -m sprintcalc '1 + 2 * 3'` prints the "
                "result. Add tests using subprocess or direct main invocation. Run tests with Bash. "
                f"Finish with a Final Summary. {FINAL_SUMMARY_ACTIVITY_RULE}"
            ),
        },
        {
            "id": "batch-mode",
            "prompt": (
                "Task 4/5: Continue the same project. Add batch file support to the CLI: ignore "
                "blank lines and # comments, evaluate each expression, and report invalid lines "
                "without stopping later lines. Add tests and run them with Bash. Finish with a "
                f"Final Summary. {FINAL_SUMMARY_ACTIVITY_RULE}"
            ),
        },
        {
            "id": "agent-review",
            "prompt": (
                "Task 5/5: Continue the same project. First launch one background Agent with "
                "run_in_background=true to review the calculator project for release risks. Then "
                "call TaskOutput for the returned task_id. Use the child result to implement one "
                "concrete improvement, update tests or docs, run tests with Bash, and finish with "
                "a Final Summary. Do not claim the child completed until TaskOutput returns. "
                f"{FINAL_SUMMARY_ACTIVITY_RULE}"
            ),
        },
    ]
    while len(base) < min_tasks:
        idx = len(base) + 1
        base.append(
            {
                "id": f"stability-{idx}",
                "prompt": (
                    f"Task {idx}/{min_tasks}: Continue the same project. Find one small but real "
                    "quality improvement, implement it with tests or documentation, run tests with "
                    f"Bash, and finish with a Final Summary. {FINAL_SUMMARY_ACTIVITY_RULE}"
                ),
            }
        )
    return base[:min_tasks]


def append_jsonl(path: Path, payload: dict[str, Any]) -> None:
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(payload, ensure_ascii=False) + "\n")


def text_message(role: str, text: str) -> dict[str, Any]:
    return {
        "role": role,
        "content": [{"type": "text", "text": text}],
        "timestamp": now_iso(),
    }


def seed_resume_transcript(
    *,
    home: Path,
    work_dir: Path,
    model: str,
    history: list[dict[str, str]],
) -> str:
    session_id = f"release-coding-sprint-{len(history)}-{uuid.uuid4().hex[:8]}"
    transcript_dir = home / ".mossen" / "transcripts"
    transcript_dir.mkdir(parents=True, exist_ok=True)
    messages: list[dict[str, Any]] = []
    for item in history:
        messages.append(text_message("user", item["prompt"]))
        messages.append(text_message("assistant", item["summary"]))
    payload = {
        "session_id": session_id,
        "messages": messages,
        "message_count": len(messages),
        "created": now_iso(),
        "updated": now_iso(),
        "model": model,
        "cwd": str(work_dir),
    }
    (transcript_dir / f"{session_id}.json").write_text(
        json.dumps(payload, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    return session_id


def command_for_task(prompt: str, work_dir: Path, restore_id: str | None, turn_limit: int) -> list[str]:
    command = [
        str(ROOT / "scripts" / "start-mossen.sh"),
        "--oneshot",
        prompt,
        "--emit",
        "stream-json",
        "--access-policy",
        "unrestricted",
        "--instruments",
        INSTRUMENTS,
        "--turn-limit",
        str(turn_limit),
        "--cwd",
        str(work_dir),
    ]
    if restore_id:
        command.extend(["--restore-id", restore_id])
    return command


def run_runner_validation(
    *,
    task_id: str,
    work_dir: Path,
    transcript_path: Path,
    timeout_secs: int,
) -> dict[str, Any]:
    command = "PYTHONPATH=. python3 -m pytest tests/ -v"
    started = time.time()
    proc = subprocess.run(
        command,
        cwd=str(work_dir),
        shell=True,
        text=True,
        capture_output=True,
        timeout=timeout_secs,
        check=False,
    )
    elapsed = time.time() - started
    content = {
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "exit_code": proc.returncode,
        "timed_out": False,
        "interrupted": False,
        "runner_enforced": True,
        "elapsed_secs": round(elapsed, 3),
    }
    encoded = json.dumps(content, ensure_ascii=False)
    append_jsonl(
        transcript_path,
        {
            "type": "tool_use_summary",
            "runner_task_id": task_id,
            "tool_name": "Bash",
            "tool_use_id": f"runner-validation-{task_id}",
            "summary": encoded,
            "full_content": encoded,
        },
    )
    return {
        "command": command,
        "exit_code": proc.returncode,
        "elapsed_secs": round(elapsed, 3),
        "stdout_tail": proc.stdout[-4000:],
        "stderr_tail": proc.stderr[-4000:],
        "ok": proc.returncode == 0,
    }


def run_task(
    *,
    task: dict[str, str],
    env: dict[str, str],
    work_dir: Path,
    home: Path,
    transcript_path: Path,
    restore_id: str | None,
    history: list[dict[str, str]],
    timeout_secs: int,
    turn_limit: int,
) -> dict[str, Any]:
    command = command_for_task(task["prompt"], work_dir, restore_id, turn_limit)
    started = time.time()
    append_jsonl(
        transcript_path,
        {
            "type": "runner_task_start",
            "task_id": task["id"],
            "timestamp": now_iso(),
            "command": command,
            "resume_note": "resume path exercised via --restore-id" if restore_id else None,
        },
    )
    proc = subprocess.run(
        command,
        cwd=str(work_dir),
        env=env,
        text=True,
        capture_output=True,
        timeout=timeout_secs,
        check=False,
    )
    elapsed = time.time() - started
    for stream_name, text in [("stdout", proc.stdout), ("stderr", proc.stderr)]:
        for line in text.splitlines():
            if not line.strip():
                continue
            try:
                payload = json.loads(line)
                if isinstance(payload, dict):
                    payload = {"runner_task_id": task["id"], **payload}
                else:
                    payload = {"type": "runner_text", "stream": stream_name, "text": line}
            except json.JSONDecodeError:
                payload = {"type": "runner_text", "stream": stream_name, "text": line}
            append_jsonl(transcript_path, payload)
    summary_text = extract_task_summary(proc.stdout)
    terminal = extract_task_terminal(proc.stdout)
    status = terminal_status(proc.returncode, terminal)
    validation: dict[str, Any] | None = None
    if status == "completed":
        validation = run_runner_validation(
            task_id=task["id"],
            work_dir=work_dir,
            transcript_path=transcript_path,
            timeout_secs=min(timeout_secs, 180),
        )
        if not validation.get("ok"):
            status = "validation_failed"
    history.append(
        {
            "prompt": task["prompt"],
            "summary": summary_text or proc.stdout[-2000:] or f"Task {task['id']} exited {proc.returncode}.",
        }
    )
    next_restore_id = seed_resume_transcript(
        home=home,
        work_dir=work_dir,
        model=env["MOSSEN_CODE_CUSTOM_MODEL"],
        history=history,
    )
    append_jsonl(
        transcript_path,
        {
            "type": "runner_task_finish",
            "task_id": task["id"],
            "timestamp": now_iso(),
            "exit_code": proc.returncode,
            "terminal": terminal,
            "status": status,
            "elapsed_secs": round(elapsed, 3),
            "runner_validation": validation,
            "restore_id": next_restore_id,
        },
    )
    return {
        "id": task["id"],
        "status": status,
        "exit_code": proc.returncode,
        "terminal": terminal,
        "elapsed_secs": round(elapsed, 3),
        "runner_validation": validation,
        "restore_id": next_restore_id,
        "stdout_tail": proc.stdout[-4000:],
        "stderr_tail": proc.stderr[-4000:],
    }


def extract_task_summary(stdout: str) -> str:
    for line in reversed(stdout.splitlines()):
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(payload, dict):
            continue
        if payload.get("type") == "assistant":
            text = json.dumps(payload.get("message") or payload, ensure_ascii=False)
            if "Final Summary" in text:
                return text[-4000:]
        if payload.get("type") == "result":
            text = json.dumps(payload, ensure_ascii=False)
            if "Final Summary" in text:
                return text[-4000:]
    if "Final Summary" in stdout:
        return stdout[stdout.rfind("Final Summary") :][-4000:]
    return stdout[-2000:]


def extract_task_terminal(stdout: str) -> str | None:
    terminal: str | None = None
    for line in stdout.splitlines():
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(payload, dict):
            continue
        if payload.get("type") == "result":
            value = payload.get("terminal")
            if isinstance(value, str) and value.strip():
                terminal = value.strip()
    return terminal


def terminal_status(returncode: int, terminal: str | None) -> str:
    if returncode != 0:
        return "failed"
    if terminal == "Completed":
        return "completed"
    if terminal and "MaxTurns" in terminal:
        return "turn_limit"
    if terminal:
        return "incomplete"
    return "missing_result"


def transcript_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def prepare_input_responsiveness_evidence(
    *,
    args: argparse.Namespace,
    artifact_dir: Path,
) -> Path | None:
    target = artifact_dir / "input-responsive-evidence.json"
    if args.input_responsive_evidence:
        payload = load_json(args.input_responsive_evidence)
        if not payload:
            raise SystemExit(
                f"input responsiveness evidence is missing or invalid JSON: {args.input_responsive_evidence}"
            )
        payload.setdefault("source_path", str(args.input_responsive_evidence))
        target.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        return target

    if not args.observed_input_responsive:
        return None

    note = (args.input_responsive_note or "").strip()
    if not note:
        raise SystemExit(
            "--observed-input-responsive requires --input-responsive-note or --input-responsive-evidence"
        )
    payload = {
        "ok": True,
        "method": "manual_terminal_observation",
        "observed_at": now_iso(),
        "during_background_agent_work": True,
        "observations": [
            {
                "action": "typed in the terminal while background Agent work was running",
                "result": note,
            }
        ],
    }
    target.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return target


def build_summary(
    *,
    args: argparse.Namespace,
    tasks: list[dict[str, Any]],
    transcript_path: Path,
    work_dir: Path,
    started: float,
    ended: float,
    runner_error: str | None,
    input_evidence_path: Path | None,
) -> dict[str, Any]:
    text = transcript_text(transcript_path)
    signals = transcript_signals(text)
    validation_failures = transcript_validation_failures(text)
    summary_activity_mismatches = task_summary_activity_mismatches(text)
    input_evidence = (
        input_responsiveness_evidence_status(
            {"input_responsiveness_evidence": str(input_evidence_path)},
            artifact_dir_summary_path(input_evidence_path),
        )
        if input_evidence_path
        else {"ok": False, "reason": "missing input responsiveness evidence"}
    )
    checks = {
        "background_agent_used": signals["background_agent_used"],
        "taskoutput_retrieved": signals["taskoutput_retrieved"],
        "file_edit_performed": signals["file_edit_performed"],
        "bash_command_run": signals["bash_command_run"],
        "interrupt_or_resume_exercised": signals["interrupt_or_resume_exercised"],
        "final_summary_recorded": signals["final_summary_recorded"],
        "tasks_finished_without_turn_limit": all(
            task.get("status") == "completed" for task in tasks
        ),
        "per_task_validation_bash_succeeded": not validation_failures,
        "input_responsive_during_agent_work": input_evidence["ok"],
        "input_responsiveness_evidence_recorded": input_evidence["ok"],
        "background_agent_completion_surfaced": signals["taskoutput_retrieved"],
        "final_summary_matches_activity": signals["final_summary_recorded"]
        and signals["file_edit_performed"]
        and signals["bash_command_run"]
        and not summary_activity_mismatches,
    }
    duration_minutes = (ended - started) / 60
    tasks_completed = sum(1 for task in tasks if task.get("status") == "completed")
    ok = (
        runner_error is None
        and duration_minutes >= args.minutes
        and tasks_completed >= args.min_tasks
        and all(checks.values())
    )
    return {
        "ok": ok,
        "started_at": datetime.fromtimestamp(started, timezone.utc).isoformat().replace("+00:00", "Z"),
        "ended_at": datetime.fromtimestamp(ended, timezone.utc).isoformat().replace("+00:00", "Z"),
        "duration_minutes": round(duration_minutes, 3),
        "minimum_minutes": args.minutes,
        "tasks_completed": tasks_completed,
        "minimum_tasks": args.min_tasks,
        "tasks": tasks,
        "checks": checks,
        "runner_error": runner_error,
        "summary_activity_mismatches": summary_activity_mismatches,
        "profile": args.profile,
        "work_dir": str(work_dir),
        "transcript": str(transcript_path),
        "input_responsiveness_evidence": str(input_evidence_path) if input_evidence_path else None,
        "notes": [
            "input_responsive_during_agent_work requires a valid input-responsive-evidence.json artifact.",
            "The runner uses --restore-id between tasks to exercise the resume path.",
        ],
    }


def artifact_dir_summary_path(input_evidence_path: Path | None) -> Path:
    if input_evidence_path is None:
        return DEFAULT_ARTIFACT_DIR / "summary.json"
    return input_evidence_path.parent / "summary.json"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", default="example-fast-highspeed")
    parser.add_argument("--profile-config", type=Path, default=DEFAULT_PROFILE_CONFIG)
    parser.add_argument("--artifact-dir", type=Path, default=DEFAULT_ARTIFACT_DIR)
    parser.add_argument("--minutes", type=float, default=60)
    parser.add_argument("--min-tasks", type=int, default=5)
    parser.add_argument("--task-timeout-secs", type=int, default=600)
    parser.add_argument("--turn-limit", type=int, default=32)
    parser.add_argument("--request-timeout-secs", type=int, default=180)
    parser.add_argument("--stream-timeout-secs", type=int, default=180)
    parser.add_argument("--observed-input-responsive", action="store_true")
    parser.add_argument("--input-responsive-evidence", type=Path)
    parser.add_argument("--input-responsive-note")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    artifact_dir = args.artifact_dir
    work_dir = artifact_dir / "workspace"
    home = artifact_dir / "home"
    transcript_path = artifact_dir / "transcript.jsonl"
    summary_path = artifact_dir / "summary.json"

    plan = {
        "profile": args.profile,
        "artifact_dir": str(artifact_dir),
        "work_dir": str(work_dir),
        "minutes": args.minutes,
        "min_tasks": args.min_tasks,
        "tasks": task_prompts(args.min_tasks),
        "instruments": INSTRUMENTS,
    }
    if args.dry_run:
        print(json.dumps({"dry_run": True, "plan": plan}, indent=2, ensure_ascii=False))
        return 0

    env = provider_env(args)
    if artifact_dir.exists():
        shutil.rmtree(artifact_dir)
    artifact_dir.mkdir(parents=True)
    home.mkdir(parents=True)
    (home / ".mossen").mkdir(parents=True)
    env["HOME"] = str(home)
    env["MOSSEN_CONFIG_DIR"] = str(home / ".mossen")
    env["MOSSEN_CONFIG_HOME"] = str(home / ".mossen")
    env["XDG_CONFIG_HOME"] = str(artifact_dir / "xdg")
    Path(env["XDG_CONFIG_HOME"]).mkdir(parents=True, exist_ok=True)
    input_evidence_path = prepare_input_responsiveness_evidence(
        args=args,
        artifact_dir=artifact_dir,
    )

    init_project(work_dir)
    append_jsonl(transcript_path, {"type": "runner_start", "timestamp": now_iso(), "plan": plan})

    started = time.time()
    tasks: list[dict[str, Any]] = []
    history: list[dict[str, str]] = []
    restore_id: str | None = None
    runner_error: str | None = None
    try:
        for task in task_prompts(args.min_tasks):
            result = run_task(
                task=task,
                env=env,
                work_dir=work_dir,
                home=home,
                transcript_path=transcript_path,
                restore_id=restore_id,
                history=history,
                timeout_secs=args.task_timeout_secs,
                turn_limit=args.turn_limit,
            )
            tasks.append(result)
            restore_id = result.get("restore_id") or restore_id
            if result.get("status") != "completed":
                break
        while (
            (time.time() - started) < args.minutes * 60
            and tasks
            and tasks[-1].get("status") == "completed"
        ):
            idx = len(tasks) + 1
            task = {
                "id": f"soak-followup-{idx}",
                "prompt": (
                    "Continue the same coding sprint. Run the test suite with Bash, inspect one "
                    "small area of the project, make a real improvement only if needed, and finish "
                    "with a Final Summary. Keep the project stable. "
                    f"{FINAL_SUMMARY_ACTIVITY_RULE}"
                ),
            }
            result = run_task(
                task=task,
                env=env,
                work_dir=work_dir,
                home=home,
                transcript_path=transcript_path,
                restore_id=restore_id,
                history=history,
                timeout_secs=args.task_timeout_secs,
                turn_limit=args.turn_limit,
            )
            tasks.append(result)
            restore_id = result.get("restore_id") or restore_id
            if result.get("status") != "completed":
                break
    except subprocess.TimeoutExpired as exc:
        runner_error = f"task timed out after {exc.timeout}s"
    except Exception as exc:  # pragma: no cover - defensive release artifact path
        runner_error = f"{type(exc).__name__}: {exc}"

    ended = time.time()
    append_jsonl(
        transcript_path,
        {"type": "runner_finish", "timestamp": now_iso(), "runner_error": runner_error},
    )
    summary = build_summary(
        args=args,
        tasks=tasks,
        transcript_path=transcript_path,
        work_dir=work_dir,
        started=started,
        ended=ended,
        runner_error=runner_error,
        input_evidence_path=input_evidence_path,
    )
    summary_path.write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
