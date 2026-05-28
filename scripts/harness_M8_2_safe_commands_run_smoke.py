#!/usr/bin/env python3
"""
M8.2 - current Rust visible slash commands have safe entrypoints.

This replaces the old Bun metadata probe. The Rust test executes every visible
directive with safe args in standard and internal contexts and fails on Empty,
Widget, hosted/team residue, or unfinished placeholder wording.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

REAL_HOME = Path.home()


def main() -> int:
    ctx = make_fixture("M8.2_safe_commands_current_rust")
    env = dict(ctx.env)
    env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    command = [
        "cargo",
        "test",
        "-p",
        "mossen-commands",
        "help_visible_directives_have_usable_safe_entrypoints",
    ]
    proc = subprocess.run(
        command,
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=180,
    )
    ok = proc.returncode == 0 and "test result: ok." in (proc.stdout + proc.stderr)
    write_command_log(ctx, command, proc.stdout, proc.stderr, proc.returncode)
    write_assertions(
        ctx,
        status="passed" if ok else "failed",
        assertions=[
            {
                "name": "visible_directives_have_usable_safe_entrypoints",
                "expected": True,
                "actual": ok,
                "passed": ok,
                "evidence": f"exit={proc.returncode}",
            }
        ],
    )
    print(
        json.dumps(
            {
                "test_id": ctx.test_id,
                "status": "passed" if ok else "failed",
                "passed": 1 if ok else 0,
                "total": 1,
                "fixture_root": str(ctx.root_dir),
                "design_note": "M8.2 executes current Rust visible slash commands through safe entrypoints.",
            },
            indent=2,
            ensure_ascii=False,
        )
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
