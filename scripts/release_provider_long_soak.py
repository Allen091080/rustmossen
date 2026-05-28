#!/usr/bin/env python3
"""Credentialed release soak for real custom-backend providers.

This script is intentionally outside the harness_R*/M* discovery path because it
requires real provider credentials and time. Use --dry-run to validate the plan
without sending requests.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ROOT = Path("/tmp/mossen-release-readiness")
PROTOCOLS = {"openai-compatible", "anthropic", "openai-responses"}
SCENARIOS = (
    "streaming",
    "tool_call",
    "retry",
    "cancel",
    "compact",
    "subagent",
    "resume",
)
DEFAULT_PROFILE_TEMPLATE = DEFAULT_ROOT / "provider-profiles.template.json"

PROFILE_TEMPLATES: dict[str, dict[str, Any]] = {
    "anthropic-release": {
        "provider": "anthropic",
        "baseURL": "https://api.anthropic.com",
        "model": "",
        "apiKey": "",
    },
    "openai-responses-release": {
        "provider": "openai-responses",
        "baseURL": "https://api.openai.com",
        "model": "",
        "apiKey": "",
    },
}


def default_settings_path() -> Path:
    return Path.home() / ".mossen" / "settings.json"


def redact(value: str | None) -> str | None:
    if not value:
        return value
    return "<redacted>"


def load_settings(path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {}
    except json.JSONDecodeError as exc:
        raise SystemExit(f"unable to parse profile config {path}: {exc}") from exc
    if not isinstance(payload, dict):
        return {}
    return payload


def configured_profiles(path: Path) -> dict[str, dict[str, Any]]:
    settings = load_settings(path)
    raw_profiles = settings.get("mossen.profiles") or settings.get("profiles") or {}
    if not isinstance(raw_profiles, dict):
        return {}
    profiles: dict[str, dict[str, Any]] = {}
    for name, value in raw_profiles.items():
        if isinstance(name, str) and isinstance(value, dict):
            profiles[name] = value
    return profiles


def profile_value(profile: dict[str, Any], *keys: str) -> str | None:
    for key in keys:
        value = profile.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return None


def write_profile_template(path: Path) -> dict[str, Any]:
    payload = {
        "mossen.profiles": PROFILE_TEMPLATES,
        "_instructions": [
            "Copy the wanted profile object into ~/.mossen/settings.json under mossen.profiles.",
            "Fill model and apiKey locally before running release_provider_long_soak.py.",
            "Do not commit filled credentials or paste them into bug reports.",
        ],
        "_commands": {
            "anthropic": "python3 scripts/release_provider_long_soak.py --profile anthropic-release --minutes 30",
            "openai-responses": "python3 scripts/release_provider_long_soak.py --profile openai-responses-release --minutes 30",
        },
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    return payload


def selected_profile(args: argparse.Namespace) -> dict[str, Any] | None:
    if not args.profile:
        return None
    profiles = configured_profiles(args.profile_config)
    profile = profiles.get(args.profile)
    if profile is None:
        available = ", ".join(sorted(profiles)) or "<none>"
        raise SystemExit(
            f"profile {args.profile!r} not found in {args.profile_config}; available: {available}"
        )
    return profile


def provider_env(args: argparse.Namespace) -> dict[str, str]:
    env = os.environ.copy()
    profile = selected_profile(args)
    protocol = (
        args.protocol
        or env.get("MOSSEN_RELEASE_PROTOCOL")
        or (profile_value(profile, "provider") if profile else None)
    )
    base_url = (
        args.base_url
        or env.get("MOSSEN_RELEASE_BASE_URL")
        or (profile_value(profile, "baseURL", "base_url") if profile else None)
    )
    model = (
        args.model
        or env.get("MOSSEN_RELEASE_MODEL")
        or (profile_value(profile, "model") if profile else None)
    )
    api_key = (
        args.api_key
        or env.get("MOSSEN_RELEASE_API_KEY")
        or (profile_value(profile, "apiKey", "api_key") if profile else None)
    )
    auth_token = (
        args.auth_token
        or env.get("MOSSEN_RELEASE_AUTH_TOKEN")
        or (profile_value(profile, "authToken", "auth_token") if profile else None)
    )

    missing = [
        name
        for name, value in [
            ("protocol", protocol),
            ("base-url", base_url),
            ("model", model),
        ]
        if not value
    ]
    if not api_key and not auth_token:
        missing.append("api-key or auth-token")
    if missing:
        raise SystemExit(f"missing required provider config: {', '.join(missing)}")
    if protocol not in PROTOCOLS:
        raise SystemExit(f"unsupported protocol: {protocol}")

    env.update(
        {
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": protocol,
            "MOSSEN_CODE_CUSTOM_BASE_URL": base_url,
            "MOSSEN_CODE_CUSTOM_MODEL": model,
            "MOSSEN_CODE_CUSTOM_NAME": f"release-soak-{protocol}",
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


def scenario_for_attempt(attempt: int) -> str:
    return SCENARIOS[(attempt - 1) % len(SCENARIOS)]


def marker_for_scenario(scenario: str) -> str:
    return f"release-soak-{scenario.replace('_', '-')}-ok"


def scenario_prompt(scenario: str) -> str:
    marker = marker_for_scenario(scenario)
    if scenario == "streaming":
        return f"No tools are available. Reply exactly with {marker} and no other text."
    if scenario == "tool_call":
        return (
            "Use one available read-only file discovery or read tool on Cargo.toml, "
            f"then reply exactly with {marker} and no other text."
        )
    if scenario == "retry":
        return (
            "This request intentionally uses a tiny timeout to exercise retry handling. "
            f"If it completes, reply exactly with {marker} and no other text."
        )
    if scenario == "cancel":
        return (
            "Write a long answer with one numbered line at a time until interrupted. "
            "Do not stop early."
        )
    if scenario == "compact":
        return f"After any pending compaction is handled, reply exactly with {marker}."
    if scenario == "subagent":
        return (
            "Launch one background Agent with run_in_background=true. Its prompt should "
            "ask the child to reply exactly release-soak-child-ok. Then call TaskOutput "
            f"for the returned task_id and reply exactly with {marker}."
        )
    if scenario == "resume":
        return f"Using the restored prior context, reply exactly with {marker}."
    raise ValueError(f"unknown scenario: {scenario}")


def instrument_args_for_scenario(scenario: str) -> list[str]:
    if scenario == "tool_call":
        return ["--instruments", "Glob,Read,Grep"]
    if scenario == "subagent":
        return ["--instruments", "Agent,TaskOutput"]
    return ["--instruments", "__none__"]


def turn_limit_for_scenario(scenario: str) -> str:
    if scenario == "tool_call":
        return "4"
    if scenario == "subagent":
        return "8"
    return "1"


def base_command(
    scenario: str,
    *,
    prompt: str | None = None,
    restore_id: str | None = None,
) -> list[str]:
    prompt = prompt if prompt is not None else scenario_prompt(scenario)
    restore_args = ["--restore-id", restore_id] if restore_id else []
    return [
        str(ROOT / "scripts" / "start-mossen.sh"),
        "--oneshot",
        prompt,
        "--emit",
        "stream-json",
        *instrument_args_for_scenario(scenario),
        *restore_args,
        "--turn-limit",
        turn_limit_for_scenario(scenario),
        "--cwd",
        str(ROOT),
    ]


def command_for_attempt(_args: argparse.Namespace, attempt: int) -> list[str]:
    scenario = scenario_for_attempt(attempt)
    return base_command(scenario)


def decode_timeout_output(value: bytes | str | None) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return value


def scenario_env(env: dict[str, str], scenario: str) -> dict[str, str]:
    scenario_env = dict(env)
    if scenario == "retry":
        scenario_env["MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS"] = "1"
        scenario_env["MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS"] = "1"
    return scenario_env


def compact_control_request(attempt: int) -> str:
    return (
        json.dumps(
            {
                "type": "control_request",
                "request_id": f"release-compact-{attempt}",
                "request": {
                    "subtype": "compact_conversation",
                    "mode": "manual",
                    "dry_run": False,
                    "custom_instructions": "Keep release soak markers intact.",
                },
            },
            ensure_ascii=False,
        )
        + "\n"
    )


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def text_message(role: str, text: str) -> dict[str, Any]:
    return {
        "role": role,
        "content": [{"type": "text", "text": text}],
        "timestamp": now_iso(),
    }


def seed_transcript(env: dict[str, str], scenario: str, attempt: int) -> str:
    session_id = f"release-soak-{scenario}-{attempt}-{uuid.uuid4().hex[:8]}"
    home = Path(env["HOME"])
    transcript_dir = home / ".mossen" / "transcripts"
    transcript_dir.mkdir(parents=True, exist_ok=True)
    messages: list[dict[str, Any]] = []
    for idx in range(4):
        messages.append(
            text_message(
                "user",
                f"Prior {scenario} user turn {idx}. Preserve marker release-soak-history-{idx}.",
            )
        )
        messages.append(
            text_message(
                "assistant",
                f"Prior {scenario} assistant turn {idx}. release-soak-history-{idx}.",
            )
        )
    payload = {
        "session_id": session_id,
        "messages": messages,
        "message_count": len(messages),
        "created": now_iso(),
        "updated": now_iso(),
        "model": env["MOSSEN_CODE_CUSTOM_MODEL"],
        "cwd": str(ROOT),
    }
    (transcript_dir / f"{session_id}.json").write_text(
        json.dumps(payload, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    return session_id


def retry_signal(stdout: str, stderr: str) -> bool:
    combined = f"{stdout}\n{stderr}".lower()
    return "api retry" in combined or "retry" in combined


def scenario_ok(
    scenario: str,
    *,
    exit_code: int | None,
    timed_out: bool,
    stdout: str,
    stderr: str,
    marker: str,
    terminated_by_cancel: bool = False,
) -> tuple[bool, dict[str, Any]]:
    marker_seen = marker in stdout
    tool_signal_seen = scenario != "tool_call" or "Glob" in stdout or "tool_use" in stdout
    retry_seen = retry_signal(stdout, stderr)
    compact_signal_seen = scenario != "compact" or "compact_request_status" in stdout
    subagent_signal_seen = scenario != "subagent" or (
        "TaskOutput" in stdout or "async_launched" in stdout or marker_seen
    )
    resume_signal_seen = scenario != "resume" or marker_seen

    if scenario == "retry":
        ok = retry_seen and (timed_out or exit_code not in (0, None) or marker_seen)
    elif scenario == "cancel":
        ok = terminated_by_cancel and not timed_out
    else:
        ok = (
            exit_code == 0
            and marker_seen
            and tool_signal_seen
            and compact_signal_seen
            and subagent_signal_seen
            and resume_signal_seen
        )
    return ok, {
        "marker_seen": marker_seen,
        "tool_signal_seen": tool_signal_seen,
        "retry_signal_seen": retry_seen,
        "compact_signal_seen": compact_signal_seen,
        "subagent_signal_seen": subagent_signal_seen,
        "resume_signal_seen": resume_signal_seen,
        "terminated_by_cancel": terminated_by_cancel,
    }


def run_command_capture(
    command: list[str],
    env: dict[str, str],
    timeout: int,
    stdin_text: str | None = None,
) -> tuple[str, str, int | None, bool]:
    try:
        proc = subprocess.run(
            command,
            cwd=str(ROOT),
            env=env,
            input=stdin_text,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return proc.stdout, proc.stderr, proc.returncode, False
    except subprocess.TimeoutExpired as exc:
        return decode_timeout_output(exc.stdout), decode_timeout_output(exc.stderr), 124, True


def run_attempt(
    command: list[str],
    env: dict[str, str],
    timeout: int,
    attempt: int,
    cancel_after_secs: float,
) -> dict[str, Any]:
    started = time.time()
    scenario = scenario_for_attempt(attempt)
    marker = marker_for_scenario(scenario)
    env = scenario_env(env, scenario)
    stdin_text: str | None = None
    terminated_by_cancel = False

    if scenario == "compact":
        restore_id = seed_transcript(env, scenario, attempt)
        command = base_command(scenario, restore_id=restore_id)
        stdin_text = compact_control_request(attempt)
    elif scenario == "resume":
        seed_marker = "release-soak-resume-seed-ok"
        seed_command = base_command(
            "streaming",
            prompt=f"Remember the word release-soak-resume-context. Reply exactly {seed_marker}.",
        )
        seed_stdout, seed_stderr, seed_exit_code, seed_timed_out = run_command_capture(
            seed_command, env, timeout
        )
        if seed_exit_code != 0 or seed_marker not in seed_stdout:
            elapsed = time.time() - started
            return {
                "command": seed_command,
                "scenario": "resume",
                "exit_code": seed_exit_code,
                "elapsed_secs": round(elapsed, 3),
                "timed_out": seed_timed_out,
                "expected_marker": marker,
                "resume_seed_failed": True,
                "marker_seen": False,
                "stdout_tail": seed_stdout[-4000:],
                "stderr_tail": seed_stderr[-4000:],
                "ok": False,
            }
        transcript_dir = Path(env["HOME"]) / ".mossen" / "transcripts"
        latest = max(transcript_dir.glob("*.json"), key=lambda path: path.stat().st_mtime)
        command = base_command(scenario, restore_id=latest.stem)

    try:
        if scenario == "cancel":
            proc = subprocess.Popen(
                command,
                cwd=str(ROOT),
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            time.sleep(cancel_after_secs)
            if proc.poll() is None:
                proc.terminate()
                terminated_by_cancel = True
                try:
                    stdout, stderr = proc.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    stdout, stderr = proc.communicate(timeout=5)
            else:
                stdout, stderr = proc.communicate(timeout=5)
            exit_code = proc.returncode
            timed_out = False
        else:
            stdout, stderr, exit_code, timed_out = run_command_capture(
                command, env, timeout, stdin_text
            )
    except subprocess.TimeoutExpired as exc:
        stdout = decode_timeout_output(exc.stdout)
        stderr = decode_timeout_output(exc.stderr)
        exit_code = 124
        timed_out = True
    elapsed = time.time() - started
    ok, signals = scenario_ok(
        scenario,
        exit_code=exit_code,
        timed_out=timed_out,
        stdout=stdout,
        stderr=stderr,
        marker=marker,
        terminated_by_cancel=terminated_by_cancel,
    )
    return {
        "command": command,
        "scenario": scenario,
        "exit_code": exit_code,
        "elapsed_secs": round(elapsed, 3),
        "timed_out": timed_out,
        "expected_marker": marker,
        **signals,
        "stdout_tail": stdout[-4000:],
        "stderr_tail": stderr[-4000:],
        "ok": ok,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--minutes", type=float, default=30)
    parser.add_argument("--interval-secs", type=float, default=30)
    parser.add_argument("--protocol", choices=sorted(PROTOCOLS))
    parser.add_argument("--base-url")
    parser.add_argument("--model")
    parser.add_argument("--api-key")
    parser.add_argument("--auth-token")
    parser.add_argument(
        "--profile",
        help="Load protocol, base URL, model, and credential from a configured Mossen profile.",
    )
    parser.add_argument("--profile-config", type=Path, default=default_settings_path())
    parser.add_argument(
        "--list-profiles",
        action="store_true",
        help="List configured profiles in redacted form and exit.",
    )
    parser.add_argument("--request-timeout-secs", type=int, default=120)
    parser.add_argument("--stream-timeout-secs", type=int, default=120)
    parser.add_argument("--attempt-timeout-secs", type=int, default=180)
    parser.add_argument("--cancel-after-secs", type=float, default=2.0)
    parser.add_argument("--artifact-root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--write-profile-template",
        action="store_true",
        help="Write a redacted Anthropic/OpenAI Responses profile template and exit.",
    )
    parser.add_argument("--profile-template-output", type=Path, default=DEFAULT_PROFILE_TEMPLATE)
    args = parser.parse_args()

    if args.write_profile_template:
        payload = write_profile_template(args.profile_template_output)
        print(
            json.dumps(
                {
                    "ok": True,
                    "template": str(args.profile_template_output),
                    "profiles": sorted((payload.get("mossen.profiles") or {}).keys()),
                    "commands": payload.get("_commands"),
                },
                indent=2,
                ensure_ascii=False,
            )
        )
        return 0

    if args.list_profiles:
        profiles = configured_profiles(args.profile_config)
        redacted = {
            name: {
                "provider": profile_value(profile, "provider"),
                "baseURL": profile_value(profile, "baseURL", "base_url"),
                "model": profile_value(profile, "model"),
                "has_api_key": bool(profile_value(profile, "apiKey", "api_key")),
                "has_auth_token": bool(profile_value(profile, "authToken", "auth_token")),
            }
            for name, profile in sorted(profiles.items())
        }
        print(json.dumps({"profile_config": str(args.profile_config), "profiles": redacted}, indent=2))
        return 0

    env = provider_env(args)
    protocol = env["MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL"]
    out_dir = args.artifact_root / protocol
    out_dir.mkdir(parents=True, exist_ok=True)
    isolated_home = out_dir / "home"
    isolated_config = isolated_home / ".mossen"
    isolated_xdg = out_dir / "xdg"
    isolated_config.mkdir(parents=True, exist_ok=True)
    isolated_xdg.mkdir(parents=True, exist_ok=True)
    env["HOME"] = str(isolated_home)
    env["MOSSEN_CONFIG_DIR"] = str(isolated_config)
    env["MOSSEN_CONFIG_HOME"] = str(isolated_config)
    env["XDG_CONFIG_HOME"] = str(isolated_xdg)

    plan = {
        "protocol": protocol,
        "base_url": env["MOSSEN_CODE_CUSTOM_BASE_URL"],
        "model": env["MOSSEN_CODE_CUSTOM_MODEL"],
        "api_key": redact(env.get("MOSSEN_CODE_CUSTOM_API_KEY")),
        "auth_token": redact(env.get("MOSSEN_CODE_CUSTOM_AUTH_TOKEN")),
        "minutes": args.minutes,
        "interval_secs": args.interval_secs,
        "scenarios": list(SCENARIOS),
        "isolated_home": str(isolated_home),
        "artifact_dir": str(out_dir),
    }
    if args.dry_run:
        print(json.dumps({"dry_run": True, "plan": plan}, indent=2))
        return 0

    for stale in [*out_dir.glob("attempt-*.json"), out_dir / "soak-report.json"]:
        try:
            stale.unlink()
        except FileNotFoundError:
            pass

    started_at = time.time()
    minimum_duration_secs = args.minutes * 60
    deadline = started_at + minimum_duration_secs
    attempts: list[dict[str, Any]] = []
    attempt = 1
    while time.time() < deadline or attempt <= len(SCENARIOS):
        command = command_for_attempt(args, attempt)
        result = run_attempt(
            command,
            env,
            args.attempt_timeout_secs,
            attempt,
            args.cancel_after_secs,
        )
        attempts.append(result)
        (out_dir / f"attempt-{attempt:04}.json").write_text(
            json.dumps(result, indent=2, ensure_ascii=False),
            encoding="utf-8",
        )
        if not result["ok"]:
            break
        attempt += 1
        remaining = deadline - time.time()
        if remaining > 0:
            time.sleep(min(args.interval_secs, remaining))

    wall_duration_secs = time.time() - started_at
    scenarios_completed = sorted(
        {
            item.get("scenario")
            for item in attempts
            if item.get("ok") and isinstance(item.get("scenario"), str)
        }
    )
    report = {
        "plan": plan,
        "scenarios_completed": scenarios_completed,
        "started_attempts": len(attempts),
        "passed_attempts": sum(1 for item in attempts if item["ok"]),
        "failed_attempts": [idx + 1 for idx, item in enumerate(attempts) if not item["ok"]],
        "minimum_duration_secs": round(minimum_duration_secs, 3),
        "wall_duration_secs": round(wall_duration_secs, 3),
        "attempt_active_duration_secs": round(sum(item["elapsed_secs"] for item in attempts), 3),
        "ok": bool(attempts)
        and all(item["ok"] for item in attempts)
        and wall_duration_secs >= minimum_duration_secs,
    }
    (out_dir / "soak-report.json").write_text(
        json.dumps(report, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
