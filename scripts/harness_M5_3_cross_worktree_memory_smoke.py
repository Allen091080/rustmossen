#!/usr/bin/env python3
"""
M5.3 — 跨 worktree (不同 cwd) 共享 user-level memory。

按 harness全链路测试.md §3.5 M5.3 契约:
  前置: 在同一 fixture 下创建 worktree_a / worktree_b 两个目录,
        共享同一 HOME 和 MOSSEN_CONFIG_DIR (这是 user-level memory
        天然的共享方式; 不依赖真 git worktree)
  步骤 1 (worktree A): cwd=worktree_a, bun -e 把 marker
        'MOSSEN_M5_3_WORKTREE_SHARED_xyz' 写到
        $MOSSEN_CONFIG_DIR/MOSSEN.md
  步骤 2 (worktree B): cwd=worktree_b, 不同 cwd 但同 HOME / MOSSEN_CONFIG_DIR,
        独立 bun -e 调 getMemoryFiles(), 验返回 entries 中存在
        type == 'User' AND content 含 marker
  断言: User-level memory 对 cwd 无依赖, 跨 worktree 真共享。

观察点 (强契约, 任一失败 → fail):
  1. worktree A 写文件子进程退出码 0
  2. fixture user MOSSEN.md 物理存在 + 含 marker
  3. worktree B loader 子进程退出码 0
  4. loader 返回 entries 中至少 1 条 type=='User'
  5. 该 User entry content 含 marker

反测信号 (改这些位置必让此 smoke fail):
  - memdir/paths.ts 把 user-level memory 解析改成依赖 cwd
    (例如 path.join(cwd, '.mossen/MOSSEN.md')) → worktree B 不同 cwd
    解析到不同路径, 找不到 marker → fail
  - utils/mossenmd.ts 中 getMemoryFiles() 把 User 类型读取分支短路
    → loader 不返回 User entry → fail
  - 把 MOSSEN_CONFIG_DIR 解析换成 process.cwd 派生的路径 →
    worktree B 解析路径 ≠ worktree A 写入路径 → fail
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

RUN_BUN = str(ROOT / "run-bun-featured.sh")

MARKER = "MOSSEN_M5_3_WORKTREE_SHARED_xyz"


def _bun_write_user_md(env: dict, cwd: str, target_path: str) -> subprocess.CompletedProcess:
    """worktree A: bun 写 marker 到 user-level MOSSEN.md。"""
    snippet = (
        "import { promises as fs } from 'node:fs';"
        "import * as path from 'node:path';"
        f"const target = {json.dumps(target_path)};"
        f"const marker = {json.dumps(MARKER)};"
        "await fs.mkdir(path.dirname(target), { recursive: true });"
        "await fs.writeFile(target, marker + '\\n', 'utf-8');"
        "process.stdout.write(JSON.stringify({wrote: target}) + '\\n');"
    )
    return subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=cwd,
        text=True,
        capture_output=True,
        timeout=60,
        env=env,
    )


def _bun_call_loader(env: dict, cwd: str) -> tuple[dict | None, subprocess.CompletedProcess]:
    """worktree B: 独立 bun 子进程调 getMemoryFiles. 返回 (parsed, proc)."""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { getMemoryFiles } from './utils/mossenmd.ts';"
        "const files = await getMemoryFiles();"
        "process.stdout.write(JSON.stringify({"
        "  count: files.length,"
        "  entries: files.map((f: any) => ({type: f.type, path: f.path, content: f.content})),"
        "}) + '\\n');"
    )
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=cwd,
        text=True,
        capture_output=True,
        timeout=60,
        env=env,
    )
    parsed = None
    for line in reversed((proc.stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                parsed = json.loads(line)
                break
            except json.JSONDecodeError:
                continue
    return parsed, proc


def case_cross_worktree_user_memory_shared() -> dict:
    ctx = make_fixture("M5.3")

    worktree_a = ctx.root_dir / "worktree_a"
    worktree_b = ctx.root_dir / "worktree_b"
    worktree_a.mkdir(parents=True, exist_ok=True)
    worktree_b.mkdir(parents=True, exist_ok=True)

    user_md_path = ctx.mossen_config_home / "MOSSEN.md"

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    # ---- Process A (worktree_a cwd, 但 bun 仍需 ROOT 的 utils) ----
    # 关键: bun -e 从 cwd 解析模块. 我们用 'node:fs' (node 内置) 和绝对 target
    # 路径, 不依赖 mossen 源码模块, 可以从 worktree_a cwd 跑。
    pA = _bun_write_user_md(env=env, cwd=str(worktree_a), target_path=str(user_md_path))

    write_command_log(
        ctx,
        [RUN_BUN, "-e", "<write user MOSSEN.md from worktree_a>"],
        pA.stdout, pA.stderr, pA.returncode,
    )

    file_exists = user_md_path.exists()
    file_content = user_md_path.read_text(encoding="utf-8") if file_exists else ""
    file_has_marker = MARKER in file_content

    # ---- Process B (worktree_b cwd, 同 HOME / MOSSEN_CONFIG_DIR) ----
    # getMemoryFiles 需要 mossen 源码模块, 但 bun -e snippet 用相对路径
    # './utils/...' 解析依赖 cwd. 因此 cwd 必须是 ROOT —— 但这与"不同 worktree"
    # 矛盾. 解决: 让 cwd=worktree_b, snippet 用绝对路径 import。
    snippet_b = (
        f"import {{ enableConfigs }} from {json.dumps(str(ROOT / 'utils' / 'config.ts'))};"
        "enableConfigs();"
        f"import {{ getMemoryFiles }} from {json.dumps(str(ROOT / 'utils' / 'mossenmd.ts'))};"
        "const files = await getMemoryFiles();"
        "process.stdout.write(JSON.stringify({"
        "  count: files.length,"
        "  cwd: process.cwd(),"
        "  entries: files.map((f: any) => ({type: f.type, path: f.path, content: f.content})),"
        "}) + '\\n');"
    )
    pB = subprocess.run(
        [RUN_BUN, "-e", snippet_b],
        cwd=str(worktree_b),
        text=True,
        capture_output=True,
        timeout=60,
        env=env,
    )

    loader_result = None
    for line in reversed((pB.stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                loader_result = json.loads(line)
                break
            except json.JSONDecodeError:
                continue

    user_entry_with_marker = None
    if loader_result is not None:
        for entry in loader_result.get("entries", []):
            if entry.get("type") == "User" and MARKER in (entry.get("content") or ""):
                user_entry_with_marker = entry
                break

    loader_user_count = (
        sum(1 for e in (loader_result or {}).get("entries", [])
            if e.get("type") == "User")
        if loader_result is not None else 0
    )

    ok = (
        pA.returncode == 0
        and file_exists
        and file_has_marker
        and pB.returncode == 0
        and loader_result is not None
        and user_entry_with_marker is not None
    )

    return {
        "name": "M5_3_cross_worktree_user_memory_shared",
        "ok": ok,
        "worktree_a_cwd": str(worktree_a),
        "worktree_b_cwd": str(worktree_b),
        "shared_user_md_path": str(user_md_path),
        "processA_exit": pA.returncode,
        "processB_exit": pB.returncode,
        "file_exists": file_exists,
        "file_has_marker": file_has_marker,
        "loader_count": (loader_result or {}).get("count"),
        "loader_cwd_reported": (loader_result or {}).get("cwd"),
        "loader_user_count": loader_user_count,
        "loader_user_entry_with_marker_path": (
            user_entry_with_marker.get("path") if user_entry_with_marker else None
        ),
        "pA_stderr_excerpt": (pA.stderr or "")[:400],
        "pB_stderr_excerpt": (pB.stderr or "")[:400],
        "_ctx": ctx,
    }


def main() -> int:
    res = case_cross_worktree_user_memory_shared()
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
                    f"pA_exit={r.get('processA_exit')} "
                    f"pB_exit={r.get('processB_exit')} "
                    f"file_exists={r.get('file_exists')} "
                    f"file_has_marker={r.get('file_has_marker')} "
                    f"loader_count={r.get('loader_count')} "
                    f"loader_user_count={r.get('loader_user_count')} "
                    f"loader_cwd={r.get('loader_cwd_reported')} "
                    f"user_entry_path={r.get('loader_user_entry_with_marker_path')}"
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
            "M5.3: worktree_a writes user MOSSEN.md, worktree_b (different cwd, "
            "same HOME / MOSSEN_CONFIG_DIR) loader must see User entry with marker. "
            "Proves user-level memory is cwd-independent and truly cross-worktree shared."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
