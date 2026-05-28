#!/usr/bin/env python3
"""
M8.3 - current Rust side-effect command smoke.

Validates side effects through Rust tests instead of retired TS settings and
memory modules: `/config` persists to the isolated config file, `/lang` updates
config/runtime language, and memory scans see files written before a fresh load.
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

REAL_HOME = Path.home()

CHECKS = [
    (
        "config_set_persists_and_query_reads_current_rust_config",
        [
            "cargo",
            "test",
            "-p",
            "mossen-commands",
            "config_set_persists_and_query_reads_current_rust_config",
        ],
    ),
    (
        "lang_set_updates_config_and_runtime_language",
        ["cargo", "test", "-p", "mossen-commands", "lang_set_updates_config_and_runtime_language"],
    ),
    (
        "lang_clear_removes_config_and_runtime_language",
        ["cargo", "test", "-p", "mossen-commands", "lang_clear_removes_config_and_runtime_language"],
    ),
    (
        "scan_memory_files_reads_written_marker_after_restart",
        ["cargo", "test", "-p", "mossen-cli", "scan_memory_files_reads_written_marker_after_restart"],
    ),
]


def run_check(ctx, name: str, command: list[str]) -> dict[str, Any]:
    env = dict(ctx.env)
    env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    proc = subprocess.run(
        command,
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=180,
    )
    return {
        "name": name,
        "ok": proc.returncode == 0 and "test result: ok." in (proc.stdout + proc.stderr),
        "exit_code": proc.returncode,
        "command": command,
        "stdout_excerpt": proc.stdout[:1000],
        "stderr_excerpt": proc.stderr[:1000],
    }


def main() -> int:
    ctx = make_fixture("M8.3_side_effect_commands_current_rust")
    results = [run_check(ctx, name, command) for name, command in CHECKS]
    stdout = "\n\n".join(f"## {r['name']}\n{r['stdout_excerpt']}" for r in results)
    stderr = "\n\n".join(f"## {r['name']}\n{r['stderr_excerpt']}" for r in results)
    write_command_log(
        ctx,
        [" && ".join(" ".join(r["command"]) for r in results)],
        stdout,
        stderr,
        0 if all(r["ok"] for r in results) else 1,
    )
    write_assertions(
        ctx,
        status="passed" if all(r["ok"] for r in results) else "failed",
        assertions=[
            {
                "name": r["name"],
                "expected": True,
                "actual": r["ok"],
                "passed": r["ok"],
                "evidence": f"exit={r['exit_code']} command={' '.join(r['command'])}",
            }
            for r in results
        ],
    )
    summary = {
        "test_id": ctx.test_id,
        "status": "passed" if all(r["ok"] for r in results) else "failed",
        "passed": sum(1 for r in results if r["ok"]),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M8.3 validates current Rust side-effect command and memory persistence paths.",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r["ok"] for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
