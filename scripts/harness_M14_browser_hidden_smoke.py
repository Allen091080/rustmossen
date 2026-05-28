#!/usr/bin/env python3
"""
M14 - current Rust browser/Chrome surface hidden by default.

Personal builds must not expose Chrome/browser/computer-use controls unless
the user explicitly enables that integration. This uses Rust command registry
and Chrome directive tests instead of retired Bun command enumeration.
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
        "personal_help_excludes_hosted_remote_and_team_only_surfaces",
        [
            "cargo",
            "test",
            "-p",
            "mossen-commands",
            "personal_help_excludes_hosted_remote_and_team_only_surfaces",
        ],
    ),
    (
        "chrome_directive_is_hidden_by_default_and_requires_explicit_opt_in",
        [
            "cargo",
            "test",
            "-p",
            "mossen-commands",
            "chrome_directive_is_hidden_by_default_and_requires_explicit_opt_in",
        ],
    ),
    (
        "personal_default_tool_surface_excludes_unwired_optional_tools",
        [
            "cargo",
            "test",
            "-p",
            "mossen-tools",
            "personal_default_tool_surface_excludes_unwired_optional_tools",
        ],
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
    ctx = make_fixture("M14_browser_hidden_current_rust")
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
        "design_note": "M14 validates current Rust browser/Chrome surfaces are hidden by default.",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r["ok"] for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
