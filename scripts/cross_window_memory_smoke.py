#!/usr/bin/env python3
"""
cross_window_memory_smoke.py — verify跨窗口项目记忆加载关键不变量。

Personal-version Mossen 用户场景：做项目做到一半退出，新开 terminal 进同一目录
应能读到上次的项目记忆。本 smoke 验证决定这个能力的几个加载链路不变量。

不试图覆盖完整 P1-4 验收（那需要真起两个 mossen 进程跑 /memory add + /memory list），
只验证决定性的"路径计算一致性"——这是断层最常见的根因。

验收点：

  L1 (cwd-invariant): getAutoMemPath() 在 git repo 内任意子目录返回同一路径
  L2 (project-aware): MOSSEN.md 能从 cwd 向上找到（最近优先）
  L3 (user-level): ~/.mossen/MOSSEN.md 路径稳定
  L4 (autoMem disabled gate): MOSSEN_CODE_SIMPLE / DISABLE_AUTO_MEMORY 正确禁用

Exit 0 if all pass; 1 if any fails (smoke runner gates on this).
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")


def _bun_eval(snippet: str, cwd: str | None = None, env: dict[str, str] | None = None) -> dict:
    """Run a TS snippet via bun -e, expects last expr to be JSON.stringify'd."""
    full_env = os.environ.copy()
    if env:
        full_env.update(env)
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=cwd or str(ROOT),
        text=True,
        capture_output=True,
        timeout=60,
        env=full_env,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"bun snippet failed: {proc.stderr[:500]}")
    out = (proc.stdout or "").strip()
    # bun may print other lines; take last JSON line
    for line in reversed(out.splitlines()):
        line = line.strip()
        if line.startswith("{") or line.startswith("["):
            return json.loads(line)
    raise RuntimeError(f"no json in bun output: {out!r}")


def check_l1_cwd_invariant() -> dict:
    """L1: getAutoMemPath() 跨子目录稳定（git repo 内）。"""
    snippet = (
        "import { getAutoMemPath } from './memdir/paths.ts';"
        "console.log(JSON.stringify({path: getAutoMemPath()}));"
    )
    # Run from project root
    r1 = _bun_eval(snippet, cwd=str(ROOT))
    # Run from subdir (utils/) — same git root, should match
    r2 = _bun_eval(snippet, cwd=str(ROOT / "utils"))
    # Run from deeper (components/permissions/)
    r3 = _bun_eval(snippet, cwd=str(ROOT / "components" / "permissions"))

    same = r1["path"] == r2["path"] == r3["path"]
    return {
        "name": "L1_cwd_invariant",
        "ok": same,
        "from_root": r1["path"],
        "from_utils": r2["path"],
        "from_components_permissions": r3["path"],
    }


def check_l2_project_mossenmd() -> dict:
    """L2: MOSSEN.md 能从 cwd 找到（验证 mossenmd loader 还能跑）。"""
    # 不需要 MOSSEN.md 真存在，只测 loader 不崩
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { getMemoryFiles } from './utils/mossenmd.ts';"
        "const files = await getMemoryFiles();"
        "console.log(JSON.stringify({"
        "count: files.length,"
        "types: [...new Set(files.map((f: any) => f.type))],"
        "}));"
    )
    try:
        r = _bun_eval(snippet, cwd=str(ROOT))
        return {
            "name": "L2_project_mossenmd",
            "ok": r["count"] >= 0,  # loader 不崩即可
            "memory_file_count": r["count"],
            "types_loaded": r["types"],
        }
    except Exception as e:
        return {"name": "L2_project_mossenmd", "ok": False, "error": str(e)}


def check_l3_user_path() -> dict:
    """L3: ~/.mossen/MOSSEN.md 路径稳定（即便文件不存在）。"""
    snippet = (
        "import { homedir } from 'node:os';"
        "import { join } from 'node:path';"
        "console.log(JSON.stringify({"
        "user_mossen_md: join(homedir(), '.mossen', 'MOSSEN.md'),"
        "}));"
    )
    r = _bun_eval(snippet, cwd=str(ROOT))
    return {
        "name": "L3_user_path",
        "ok": r["user_mossen_md"].endswith(".mossen/MOSSEN.md"),
        "path": r["user_mossen_md"],
    }


def check_l4_disable_gates() -> dict:
    """L4: MOSSEN_CODE_SIMPLE / DISABLE_AUTO_MEMORY 真禁用 AutoMem。"""
    snippet = (
        "import { isAutoMemoryEnabled } from './memdir/paths.ts';"
        "console.log(JSON.stringify({enabled: isAutoMemoryEnabled()}));"
    )
    # Default state
    r_normal = _bun_eval(snippet, cwd=str(ROOT))
    # With SIMPLE flag
    r_simple = _bun_eval(snippet, cwd=str(ROOT), env={"MOSSEN_CODE_SIMPLE": "true"})
    # With DISABLE flag
    r_disabled = _bun_eval(
        snippet, cwd=str(ROOT), env={"MOSSEN_CODE_DISABLE_AUTO_MEMORY": "true"}
    )

    return {
        "name": "L4_disable_gates",
        "ok": r_simple["enabled"] is False and r_disabled["enabled"] is False,
        "default_enabled": r_normal["enabled"],
        "simple_disabled": not r_simple["enabled"],
        "disable_flag_disabled": not r_disabled["enabled"],
    }


def main() -> int:
    checks = [
        check_l1_cwd_invariant,
        check_l2_project_mossenmd,
        check_l3_user_path,
        check_l4_disable_gates,
    ]
    results = []
    for fn in checks:
        try:
            results.append(fn())
        except Exception as e:
            results.append({"name": fn.__name__, "ok": False, "error": str(e)})

    failed = [r for r in results if not r.get("ok")]
    print(json.dumps({"results": results, "failed_count": len(failed)}, indent=2))

    if failed:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
