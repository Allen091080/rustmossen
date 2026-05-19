#!/usr/bin/env python3
"""
FIX-GAP1 + FIX-MORE-5: 验 errorLogSink 的 USER_TYPE gate (主测) +
ant 路径下 file write (附测)。

⚠️ 命名诚实化（FIX-MORE-5）：本 smoke **主要**测 USER_TYPE gate 行为：
  - 3 case 中 2 个验"无 file write"（external default + no-throw negative）
  - 只有 1 个验"file write happens"（ant 路径附带）
  原文件名 error_boundary_filewrite 暗示主测 file write — 误导。
  改名 error_boundary_usertype_gate 反映主测内容。

🔴 调研发现：utils/errorLogSink.ts:112 `appendToLog` 在
   `USER_TYPE !== 'ant'` 时**直接 return**。Mossen personal 版 USER_TYPE='external'，
   **file write 永远不发生**（设计如此）。

契约（3 case，验 gate 真行为）：
  Case 1 (USER_TYPE=external, 默认 gate 关闭): 触发 boundary → in-memory log +1 但 file size 不变
  Case 2 (USER_TYPE=ant, gate 开启): 触发 boundary → file size 真增长 + 内容含 marker + label
  Case 3 (negative): 不 throw → file size 不变（gate 是否开启都一样）

反面案例：
  ❌ 反 1: 只测 case 2 不测 case 1 → 不证明 personal 版的"file write 不发生"是设计
  ❌ 反 2: 不注册 sink → 测试不到任何路径

User path:
  Personal 版: logError → in-memory only (errorLogSink 调但 appendToLog gate 拦)
  Ant 版: logError → file write 真发生

Mutation point:
  改 errorLogSink.ts:112 的 USER_TYPE !== 'ant' 为 USER_TYPE === 'never'
  → personal 版本 file write 也会发生 → case 1 fail (size_unchanged 变 false)
"""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")


def _bun(snippet: str, timeout: int = 60, env_override: dict | None = None) -> dict:
    env = os.environ.copy()
    if env_override:
        env.update(env_override)
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=timeout,
        env=env,
    )
    return {
        "returncode": proc.returncode,
        "stdout": proc.stdout or "",
        "stderr": proc.stderr or "",
    }


def _extract_json(out: str) -> dict | None:
    for line in reversed(out.splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return None


_TRIGGER_SNIPPET = (
    "import { enableConfigs } from './utils/config.ts';"
    "enableConfigs();"
    "import { existsSync, statSync, readFileSync } from 'node:fs';"
    "import * as React from 'react';"
    "import { render } from 'ink';"
    "import { Writable } from 'node:stream';"
    "import { initializeErrorLogSink, getErrorsPath, _flushLogWritersForTesting } from './utils/errorLogSink.ts';"
    "import { MossenErrorBoundary } from './components/MossenErrorBoundary.tsx';"
    "import { getInMemoryErrors } from './utils/log.ts';"
    "initializeErrorLogSink();"
    "const errorsPath = getErrorsPath();"
    "const sizeBefore = existsSync(errorsPath) ? statSync(errorsPath).size : 0;"
    "const contentBefore = existsSync(errorsPath) ? readFileSync(errorsPath, 'utf8') : '';"
    "const memBefore = getInMemoryErrors().length;"
    "const UNIQUE = 'GAP1FIX_FILEWRITE_' + Date.now();"
    "function Thrower() { throw new Error(UNIQUE); }"
    "const chunks: Buffer[] = [];"
    "const stdout = new Writable({write(c, _e, cb) { chunks.push(Buffer.from(c)); cb(); }});"
    "(stdout as any).isTTY = true;"
    "(stdout as any).columns = 80;"
    "(stdout as any).rows = 24;"
    "try {"
    "  const inst: any = render("
    "    React.createElement(MossenErrorBoundary, {label: 'GAP1FixLabel'},"
    "      React.createElement(Thrower)"
    "    ),"
    "    {stdout: stdout as any, exitOnCtrlC: false, patchConsole: false}"
    "  );"
    "  await new Promise(r => setTimeout(r, 300));"
    "  if (inst && typeof inst.unmount === 'function') { try { inst.unmount(); } catch {} }"
    "} catch {}"
    "_flushLogWritersForTesting();"
    "const sizeAfter = existsSync(errorsPath) ? statSync(errorsPath).size : 0;"
    "const contentAfter = existsSync(errorsPath) ? readFileSync(errorsPath, 'utf8') : '';"
    "const newContent = contentAfter.slice(contentBefore.length);"
    "const memAfter = getInMemoryErrors().length;"
    "process.stdout.write(JSON.stringify({"
    "  errors_path: errorsPath,"
    "  user_type: process.env.USER_TYPE,"
    "  size_before: sizeBefore,"
    "  size_after: sizeAfter,"
    "  size_increased: sizeAfter > sizeBefore,"
    "  delta_bytes: sizeAfter - sizeBefore,"
    "  mem_before: memBefore,"
    "  mem_after: memAfter,"
    "  mem_delta: memAfter - memBefore,"
    "  new_content_has_unique_marker: newContent.includes(UNIQUE),"
    "  new_content_has_label_prefix: newContent.includes('[MossenErrorBoundary:GAP1FixLabel]'),"
    "  new_content_has_componentStack: newContent.includes('componentStack:'),"
    "  new_content_excerpt: newContent.slice(0, 400),"
    "}) + '\\n');"
)


def case_personal_version_no_file_write_but_inmem_logged() -> dict:
    """Case 1: personal 版 (USER_TYPE=external) → in-memory log +1 但文件 size 不变。
    这是设计行为：appendToLog 在 errorLogSink.ts:112 因 USER_TYPE !== 'ant' 早 return。
    """
    r = _bun(_TRIGGER_SNIPPET, env_override={"USER_TYPE": "external"})
    if r["returncode"] != 0:
        return {"name": "personal_version_inmem_only_no_file", "ok": False,
                "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "personal_version_inmem_only_no_file", "ok": False,
                "raw": r["stdout"][:500]}
    return {
        "name": "personal_version_inmem_only_no_file",
        "ok": (
            parsed.get("user_type") == "external"
            and parsed.get("size_increased") is False  # 文件 NOT 增长
            and parsed.get("delta_bytes", 0) == 0
            and parsed.get("mem_delta", 0) >= 1  # in-memory 真增加
        ),
        **parsed,
    }


def case_ant_version_writes_to_errors_file() -> dict:
    """Case 2: USER_TYPE=ant → file write 真发生 + 内容含 marker + label。"""
    r = _bun(_TRIGGER_SNIPPET, env_override={"USER_TYPE": "ant"})
    if r["returncode"] != 0:
        return {"name": "ant_version_writes_to_errors_file", "ok": False,
                "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "ant_version_writes_to_errors_file", "ok": False,
                "raw": r["stdout"][:500]}
    return {
        "name": "ant_version_writes_to_errors_file",
        "ok": (
            parsed.get("user_type") == "ant"
            and parsed.get("size_increased") is True
            and parsed.get("delta_bytes", 0) > 0
            and parsed.get("new_content_has_unique_marker") is True
            and parsed.get("new_content_has_label_prefix") is True
            and parsed.get("mem_delta", 0) >= 1
        ),
        **parsed,
    }


def case_negative_no_boundary_no_filewrite() -> dict:
    """无 boundary throw → 文件不增长（防假阳性：可能其他来源写文件）。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { existsSync, statSync } from 'node:fs';"
        "import * as React from 'react';"
        "import { render, Text } from 'ink';"
        "import { Writable } from 'node:stream';"
        "import { initializeErrorLogSink, getErrorsPath } from './utils/errorLogSink.ts';"
        "import { MossenErrorBoundary } from './components/MossenErrorBoundary.tsx';"
        ""
        "initializeErrorLogSink();"
        "const errorsPath = getErrorsPath();"
        "const sizeBefore = existsSync(errorsPath) ? statSync(errorsPath).size : 0;"
        ""
        "function Healthy() { return React.createElement(Text, null, 'healthy'); }"
        "const chunks: Buffer[] = [];"
        "const stdout = new Writable({write(c, _e, cb) { chunks.push(Buffer.from(c)); cb(); }});"
        "(stdout as any).isTTY = true;"
        "(stdout as any).columns = 80;"
        "(stdout as any).rows = 24;"
        "const inst: any = render("
        "  React.createElement(MossenErrorBoundary, {label: 'GAP1NoThrow'},"
        "    React.createElement(Healthy)"
        "  ),"
        "  {stdout: stdout as any, exitOnCtrlC: false, patchConsole: false}"
        ");"
        "await new Promise(r => setTimeout(r, 300));"
        "if (inst && typeof inst.unmount === 'function') { try { inst.unmount(); } catch {} }"
        ""
        "const sizeAfter = existsSync(errorsPath) ? statSync(errorsPath).size : 0;"
        "process.stdout.write(JSON.stringify({"
        "  errors_path: errorsPath,"
        "  size_before: sizeBefore,"
        "  size_after: sizeAfter,"
        "  size_unchanged: sizeAfter === sizeBefore,"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "no_boundary_no_filewrite", "ok": False,
                "stderr": r["stderr"][:500], "raw": r["stdout"][:300]}
    return {
        "name": "no_boundary_no_filewrite",
        "ok": parsed.get("size_unchanged") is True,
        **parsed,
    }


def main() -> int:
    results = [
        case_personal_version_no_file_write_but_inmem_logged(),
        case_ant_version_writes_to_errors_file(),
        case_negative_no_boundary_no_filewrite(),
    ]
    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
