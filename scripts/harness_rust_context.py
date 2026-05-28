#!/usr/bin/env python3
"""Shared helpers for current Rust-backed context harness checks."""

from __future__ import annotations

import json
import os
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from harness_fixture import make_fixture, write_assertions

ROOT = Path(__file__).resolve().parents[1]
REAL_HOME = Path(os.environ.get("HOME", str(Path.home())))
COMMAND_TIMEOUT_SECS = 240


@dataclass(frozen=True)
class Step:
    name: str
    command: list[str]
    expected: tuple[str, ...] = ("test result: ok.",)
    timeout_secs: int = COMMAND_TIMEOUT_SECS


def cargo_test(*args: str) -> list[str]:
    cargo_args = list(args)
    has_target = any(
        arg in {
            "--all-targets",
            "--bin",
            "--bins",
            "--example",
            "--examples",
            "--lib",
            "--test",
            "--tests",
        }
        for arg in cargo_args
    )
    if not has_target:
        package = None
        if "-p" in cargo_args:
            package_index = cargo_args.index("-p")
            if package_index + 1 < len(cargo_args):
                package = cargo_args[package_index + 1]
        target_args = ["--bin", "mossen"] if package == "mossen-cli" else ["--lib"]
        insert_at = cargo_args.index("-p") + 2 if "-p" in cargo_args else 0
        cargo_args[insert_at:insert_at] = target_args
    return ["cargo", "test", "-q", *cargo_args, "--", "--nocapture"]


def context_env(ctx: Any) -> dict[str, str]:
    env = ctx.env.copy()
    env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    env.setdefault("MOSSEN_CONFIG_DIR", str(ctx.mossen_config_home))
    return env


def output_excerpt(text: str, limit: int = 1200) -> str:
    if len(text) <= limit:
        return text
    return text[:limit] + "\n...<truncated>..."


def artifact_name(name: str, suffix: str) -> str:
    safe = "".join(ch if ch.isalnum() else "_" for ch in name).strip("_")
    return f"{safe}.{suffix}"


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
            env=context_env(ctx),
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
        "ok": ok,
        "exit_code": exit_code,
        "timed_out": timed_out,
        "duration_secs": round(time.monotonic() - started, 3),
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


def source_check(name: str, path: str, needles: list[str]) -> dict[str, Any]:
    file_path = ROOT / path
    try:
        text = file_path.read_text(encoding="utf-8")
    except OSError as exc:
        return {
            "name": name,
            "ok": False,
            "path": path,
            "missing": needles,
            "error": str(exc),
        }
    missing = [needle for needle in needles if needle not in text]
    return {
        "name": name,
        "ok": not missing,
        "path": path,
        "missing": missing,
        "needles": needles,
    }


def write_context_report(
    ctx: Any,
    script_name: str,
    steps: list[dict[str, Any]],
    checks: list[dict[str, Any]],
    design_note: str,
) -> dict[str, str]:
    status = "passed" if all(s["ok"] for s in steps) and all(c["ok"] for c in checks) else "failed"
    report = {
        "test_id": ctx.test_id,
        "script": script_name,
        "fixture_root": str(ctx.root_dir),
        "status": status,
        "steps": steps,
        "source_checks": checks,
        "design_note": design_note,
    }
    report_path = ctx.artifacts_dir / "context-harness-report.json"
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")

    commands = [" ".join(step["command"]) for step in steps]
    stdout_summary = [f"## {step['name']}\n{step['stdout_excerpt']}" for step in steps]
    stderr_summary = [f"## {step['name']}\n{step['stderr_excerpt']}" for step in steps]
    session_lines = [json.dumps(item, ensure_ascii=False) for item in [*steps, *checks]]

    (ctx.artifacts_dir / "command.txt").write_text("\n".join(commands), encoding="utf-8")
    (ctx.artifacts_dir / "stdout.txt").write_text("\n\n".join(stdout_summary), encoding="utf-8")
    (ctx.artifacts_dir / "stderr.txt").write_text("\n\n".join(stderr_summary), encoding="utf-8")
    (ctx.artifacts_dir / "exit_code.txt").write_text("0" if status == "passed" else "1", encoding="utf-8")
    (ctx.artifacts_dir / "session_log.jsonl").write_text("\n".join(session_lines) + "\n", encoding="utf-8")
    (ctx.artifacts_dir / "env.txt").write_text(
        "\n".join(
            f"{key}={value}"
            for key, value in sorted(context_env(ctx).items())
            if key.startswith(("HOME", "MOSSEN_", "XDG_", "RUSTUP_", "CARGO_"))
        ),
        encoding="utf-8",
    )

    write_assertions(
        ctx,
        status=status,
        assertions=[
            {
                "name": item["name"],
                "expected": True,
                "actual": item["ok"],
                "passed": item["ok"],
                "evidence": json.dumps(
                    {
                        "exit_code": item.get("exit_code"),
                        "expected_ok": item.get("expected_ok"),
                        "timed_out": item.get("timed_out"),
                        "missing": item.get("missing"),
                    },
                    ensure_ascii=False,
                ),
            }
            for item in [*steps, *checks]
        ],
        extra_artifacts={"context_harness_report": str(report_path)},
    )
    return {"context_harness_report": str(report_path)}


def run_context_harness(
    test_id: str,
    script_name: str,
    steps: list[Step],
    checks: list[dict[str, Any]] | None,
    design_note: str,
) -> int:
    ctx = make_fixture(test_id)
    step_results = [run_step(ctx, step) for step in steps]
    source_checks = checks or []
    artifacts = write_context_report(ctx, script_name, step_results, source_checks, design_note)
    status = "passed" if all(s["ok"] for s in step_results) and all(c["ok"] for c in source_checks) else "failed"
    print(
        json.dumps(
            {
                "test_id": test_id,
                "status": status,
                "passed": sum(1 for item in [*step_results, *source_checks] if item["ok"]),
                "total": len(step_results) + len(source_checks),
                "fixture_root": str(ctx.root_dir),
                "artifacts": artifacts,
                "design_note": design_note,
            },
            indent=2,
            ensure_ascii=False,
        )
    )
    return 0 if status == "passed" else 1
