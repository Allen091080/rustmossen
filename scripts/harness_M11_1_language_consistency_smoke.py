#!/usr/bin/env python3
"""
M11.1 - current Rust language consistency smoke.

The Rust path reads persisted language preferences directly and `/lang` updates
both runtime vars and config, so this smoke runs the current Rust tests that
cover normalization, persisted preference, runtime override, and `/lang`
mutations.
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
        "normalizes_language_aliases_and_reads_legacy_language_setting",
        [
            "cargo",
            "test",
            "-p",
            "mossen-utils",
            "normalizes_language_aliases_and_reads_legacy_language_setting",
        ],
    ),
    (
        "i18n_uses_persisted_interactive_language_preference",
        ["cargo", "test", "-p", "mossen-utils", "i18n_uses_persisted_interactive_language_preference"],
    ),
    (
        "i18n_env_language_overrides_persisted_preference",
        ["cargo", "test", "-p", "mossen-utils", "i18n_env_language_overrides_persisted_preference"],
    ),
    (
        "lang_set_updates_config_and_runtime_language",
        ["cargo", "test", "-p", "mossen-commands", "lang_set_updates_config_and_runtime_language"],
    ),
    (
        "lang_usage_shows_explicit_preference",
        ["cargo", "test", "-p", "mossen-commands", "lang_usage_shows_explicit_preference"],
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
    ctx = make_fixture("M11.1_language_consistency_current_rust")
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
        "design_note": "M11.1 validates current Rust language preference and /lang command plumbing.",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r["ok"] for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
