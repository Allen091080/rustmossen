#!/usr/bin/env python3
"""M17.1 - standard capability evidence for TUI rendering interaction gates."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions


WRAPPED_SMOKES = [
    "scripts/wave_w278_terminal_cleanup_balance_pty_contract_smoke.py",
    "scripts/wave_w306_terminal_render_product_acceptance_gate_smoke.py",
    "scripts/wave_w320_terminal_no_fullscreen_clear_external_pty_contract_smoke.py",
]


def run_smoke(ctx, script: str) -> dict:
    command = ["python3", str(ROOT / script)]
    proc = subprocess.run(
        command,
        cwd=str(ROOT),
        env=ctx.env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=60,
    )
    safe_name = Path(script).stem
    stdout_path = ctx.artifacts_dir / f"{safe_name}.stdout.txt"
    stderr_path = ctx.artifacts_dir / f"{safe_name}.stderr.txt"
    stdout_path.write_text(proc.stdout, encoding="utf-8")
    stderr_path.write_text(proc.stderr, encoding="utf-8")
    return {
        "script": script,
        "command": " ".join(command),
        "exit_code": proc.returncode,
        "ok": proc.returncode == 0,
        "stdout": str(stdout_path),
        "stderr": str(stderr_path),
        "stdout_excerpt": proc.stdout[:500],
        "stderr_excerpt": proc.stderr[:500],
    }


def main() -> int:
    ctx = make_fixture("M17.1")
    results = [run_smoke(ctx, script) for script in WRAPPED_SMOKES]
    commands_path = ctx.artifacts_dir / "wrapped_commands.json"
    commands_path.write_text(json.dumps(results, indent=2, ensure_ascii=False), encoding="utf-8")

    write_assertions(
        ctx,
        status="passed" if all(item["ok"] for item in results) else "failed",
        assertions=[
            {
                "name": f"{Path(item['script']).stem}_exits_zero",
                "expected": 0,
                "actual": item["exit_code"],
                "passed": item["ok"],
                "evidence": (
                    f"command={item['command']} "
                    f"stdout={item['stdout']} stderr={item['stderr']}"
                ),
            }
            for item in results
        ],
        extra_artifacts={"wrapped_commands": str(commands_path)},
    )

    summary = {
        "test_id": ctx.test_id,
        "status": "passed" if all(item["ok"] for item in results) else "failed",
        "passed": sum(1 for item in results if item["ok"]),
        "total": len(results),
        "wrapped": [item["script"] for item in results],
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M17.1 turns source-level W278/W306/W320 rendering gates into "
            "standard assertions.json evidence for the capability matrix."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(item["ok"] for item in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
