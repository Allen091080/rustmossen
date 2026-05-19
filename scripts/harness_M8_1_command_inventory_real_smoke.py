#!/usr/bin/env python3
"""
M8.1 — 101 个/真注册 45 个 slash command 入口清单 (W4 修正后).

按 harness全链路测试.md §C.1 + §C.3 契约:
  matrix `harness_slash_command_matrix.json` 列出 45 个真注册命令 (从 commands.ts
  getCommands() 调用得到, 已过滤 hosted/console-only). M8.1 验 runtime 真实
  registry 与 matrix 完全一致 (count+names), 防漂移.

观察点 (强契约):
  1. bun -e 调 getCommands() 返回 commands array
  2. 数量 == matrix['total'] (45)
  3. set([cmd.name for cmd in registered]) == set([entry['command'] for entry in matrix])

反测信号: 改 src/commands.ts 注释一个 register 行 → name set 不匹配 → fail
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_BUN = str(ROOT / "run-bun-featured.sh")
MATRIX_FILE = ROOT / "harness_slash_command_matrix.json"


def case_command_inventory_matches_matrix() -> dict:
    ctx = make_fixture("M8.1")

    matrix = json.loads(MATRIX_FILE.read_text(encoding="utf-8"))
    expected_count = matrix["total"]
    expected_names = sorted(e["command"] for e in matrix["entries"])

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const { getCommands } = await import('./commands.ts');"
        "const cmds = await getCommands();"
        "process.stdout.write(JSON.stringify({"
        "  count: cmds.length,"
        "  names: cmds.map(c => c.name).sort()"
        "}) + '\\n');"
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=120,
        env=env,
    )

    write_command_log(ctx, [RUN_BUN, "-e", "<getCommands>"], proc.stdout, proc.stderr, proc.returncode)

    parsed = None
    for line in reversed((proc.stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                parsed = json.loads(line)
                break
            except json.JSONDecodeError:
                continue

    if not parsed:
        return {
            "name": "command_inventory_matches_matrix",
            "ok": False,
            "exit_code": proc.returncode,
            "stderr_excerpt": proc.stderr[:500],
            "stdout_excerpt": proc.stdout[:500],
            "_ctx": ctx,
        }

    runtime_count = parsed.get("count", 0)
    runtime_names = sorted(parsed.get("names") or [])

    count_match = runtime_count == expected_count
    names_match = runtime_names == expected_names
    missing = sorted(set(expected_names) - set(runtime_names))
    extra = sorted(set(runtime_names) - set(expected_names))

    return {
        "name": "command_inventory_matches_matrix",
        "ok": (proc.returncode == 0 and count_match and names_match),
        "exit_code": proc.returncode,
        "expected_count": expected_count,
        "runtime_count": runtime_count,
        "missing_in_runtime": missing,
        "extra_in_runtime": extra,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_command_inventory_matches_matrix()
    ctx = res.pop("_ctx")
    results = [res]

    write_assertions(
        ctx,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {
                "name": r["name"],
                "expected": True,
                "actual": r.get("ok"),
                "passed": r.get("ok"),
                "evidence": (
                    f"expected={r.get('expected_count')} runtime={r.get('runtime_count')} "
                    f"missing={r.get('missing_in_runtime')} extra={r.get('extra_in_runtime')}"
                ),
            }
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M8.1: matrix `harness_slash_command_matrix.json` 与 runtime "
            "`getCommands()` 必须完全一致 (count+name set)。防 mossen 注册漂移。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
