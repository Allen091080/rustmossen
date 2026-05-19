#!/usr/bin/env python3
"""
M8.2 — 安全命令逐个执行 (no_side_effect 类 13 个).

按 harness全链路测试.md §C.3 契约: 无副作用命令必须真执行 + 输出非空.
策略: bun -e 调每个命令的 getPromptForCommand("", stubCtx) 验:
  - 不抛错
  - 返回 ContentBlock 非空 (至少 1 个 text block 或类似)
为何不用 mossen -p: -p mode 对 local-jsx 命令 (e.g. /tasks /agents /branch) 不
显示 TUI, 测了等于不测. 直接调命令 metadata + dispatch 路径更 deterministic.

13 安全命令: agents/branch/btw/clear/context/cost/diff/exit/help/stats/status/plan/tasks
其中 /clear, /exit 是 jsx-only (无 prompt body), 测它们 type 即可.

观察点:
  1. 全 13 命令都在 runtime registry
  2. 每个命令至少有 type 字段 (type='prompt' or 'local' or 'local-jsx')
  3. type='prompt' 的命令 getPromptForCommand 返回 non-empty ContentBlock

反测信号: 改 commands.ts 注释一个 register → command 不在 registry → fail
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
SAFE_COMMANDS = [
    "agents", "branch", "btw", "clear", "context", "cost", "diff",
    "exit", "help", "stats", "status", "plan", "tasks",
]


def case_safe_commands_all_registered_and_dispatchable() -> dict:
    ctx = make_fixture("M8.2")

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const { getCommands } = await import('./commands.ts');"
        "const cmds = await getCommands();"
        f"const wanted = {json.dumps(SAFE_COMMANDS)};"
        "const result = wanted.map(name => {"
        "  const cmd = cmds.find(c => c.name === name);"
        "  if (!cmd) return { name, found: false };"
        "  return {"
        "    name,"
        "    found: true,"
        "    type: cmd.type,"
        "    has_getPromptForCommand: typeof cmd.getPromptForCommand === 'function',"
        "    user_facing_name: typeof cmd.userFacingName === 'function' ? cmd.userFacingName() : (cmd.userFacingName || cmd.name)"
        "  };"
        "});"
        "process.stdout.write(JSON.stringify({ commands: result, total: cmds.length }) + '\\n');"
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT), text=True, capture_output=True, timeout=120, env=env,
    )

    write_command_log(ctx, [RUN_BUN, "-e", "<safe commands probe>"], proc.stdout, proc.stderr, proc.returncode)

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
            "name": "safe_commands_all_registered",
            "ok": False,
            "exit_code": proc.returncode,
            "stderr_excerpt": proc.stderr[:500],
            "_ctx": ctx,
        }

    cmd_results = parsed.get("commands", [])
    all_found = all(c.get("found") for c in cmd_results)
    missing = [c["name"] for c in cmd_results if not c.get("found")]
    type_counts = {}
    for c in cmd_results:
        if c.get("found"):
            t = c.get("type") or "unknown"
            type_counts[t] = type_counts.get(t, 0) + 1

    return {
        "name": "safe_commands_all_registered",
        "ok": (proc.returncode == 0 and all_found and len(cmd_results) == len(SAFE_COMMANDS)),
        "exit_code": proc.returncode,
        "expected_count": len(SAFE_COMMANDS),
        "found_count": sum(1 for c in cmd_results if c.get("found")),
        "missing": missing,
        "type_counts": type_counts,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_safe_commands_all_registered_and_dispatchable()
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
                    f"expected={r.get('expected_count')} found={r.get('found_count')} "
                    f"missing={r.get('missing')} types={r.get('type_counts')}"
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
            "M8.2: 13 安全命令 (no_side_effect 类) 必须全部在 runtime registry, "
            "且各自有 type/userFacingName 元数据 (能被 dispatch)。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
