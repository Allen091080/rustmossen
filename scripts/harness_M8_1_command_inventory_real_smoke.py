#!/usr/bin/env python3
"""
M8.1 - current Rust slash command inventory smoke.

This smoke uses package tests that exercise the real visible directive list,
aliases, and representative index directives from the `mossen-commands`
registry.
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
        "help_resolves_every_visible_command_and_alias",
        ["cargo", "test", "-p", "mossen-commands", "help_resolves_every_visible_command_and_alias"],
    ),
    (
        "representative_index_directives_execute_real_commands",
        [
            "cargo",
            "test",
            "-p",
            "mossen-commands",
            "representative_index_directives_execute_real_commands",
        ],
    ),
    (
        "help_for_specific_command_uses_registered_metadata",
        ["cargo", "test", "-p", "mossen-commands", "help_for_specific_command_uses_registered_metadata"],
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
    ctx = make_fixture("M8.1_command_inventory_current_rust")
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
        "design_note": "M8.1 validates the current Rust slash command inventory and index directives.",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r["ok"] for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
