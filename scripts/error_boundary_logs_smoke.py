#!/usr/bin/env python3
"""
GAP 1: 验 boundary 触发时 logError 真被调用。

契约（5 条 falsifiable）：
  1. 触发前 getInMemoryErrors().length = N
  2. 触发后 getInMemoryErrors().length = N+1
  3. 新条目内容包含 throw 的 message 文本
  4. 新条目内容包含 [MossenErrorBoundary:<label>] 前缀
  5. mutation: 注释 componentDidCatch 内的 logError(...) 后测试 fail

反面案例（如何虚假糊弄）：
  ❌ 反 1: 只检查 logError 函数 import 成功 — 不证明真被调用
  ❌ 反 2: mock logError 然后断言被调用 — boundary 可能没 wired up

User path:
  React render-time throw → boundary.componentDidCatch → logError() →
  addToInMemoryErrorLog() → getInMemoryErrors() 可读

Import 自检:
  ✅ from MossenErrorBoundary (mossen 真组件)
  ✅ from utils/log (mossen 真 logger)
  ❌ NOT from node:fs (raw fs)
"""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")


def _bun(snippet: str, timeout: int = 60) -> dict:
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=timeout,
        env=os.environ.copy(),
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


def case_logError_called_when_boundary_triggers() -> dict:
    """触发 boundary，验证 in-memory error log 增长 + 内容匹配。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import * as React from 'react';"
        "import { render } from 'ink';"
        "import { Writable } from 'node:stream';"
        "import { MossenErrorBoundary } from './components/MossenErrorBoundary.tsx';"
        "import { getInMemoryErrors } from './utils/log.ts';"
        ""
        "const errorsBefore = getInMemoryErrors().length;"
        ""
        "const UNIQUE_THROW_MSG = 'GAP1_UNIQUE_THROW_MSG_' + Date.now();"
        "function Thrower() { throw new Error(UNIQUE_THROW_MSG); }"
        ""
        "const chunks: Buffer[] = [];"
        "const stdout = new Writable({write(chunk, _e, cb) { chunks.push(Buffer.from(chunk)); cb(); }});"
        "(stdout as any).isTTY = true;"
        "(stdout as any).columns = 80;"
        "(stdout as any).rows = 24;"
        ""
        "let render_threw = false;"
        "try {"
        "  const inst: any = render(React.createElement(MossenErrorBoundary, {label: 'GAP1Test'}, React.createElement(Thrower)), {stdout: stdout as any, exitOnCtrlC: false, patchConsole: false});"
        "  await new Promise(r => setTimeout(r, 200));"
        "  if (inst && typeof inst.unmount === 'function') { try { inst.unmount(); } catch {} }"
        "} catch { render_threw = true; }"
        ""
        "const errorsAfter = getInMemoryErrors();"
        "const newEntries = errorsAfter.slice(errorsBefore);"
        "const matchingEntry = newEntries.find((e: any) => e.error.includes(UNIQUE_THROW_MSG));"
        ""
        "process.stdout.write(JSON.stringify({"
        "  errors_before: errorsBefore,"
        "  errors_after: errorsAfter.length,"
        "  delta_count: errorsAfter.length - errorsBefore,"
        "  unique_msg_logged: matchingEntry !== undefined,"
        "  has_boundary_label_prefix: matchingEntry ? matchingEntry.error.includes('[MossenErrorBoundary:GAP1Test]') : false,"
        "  has_unique_throw_msg: matchingEntry ? matchingEntry.error.includes(UNIQUE_THROW_MSG) : false,"
        "  has_stack_trace: matchingEntry ? matchingEntry.error.includes('at Thrower') : false,"
        "  has_component_stack: matchingEntry ? matchingEntry.error.includes('componentStack:') : false,"
        "  has_iso_timestamp: matchingEntry ? /^\\d{4}-\\d{2}-\\d{2}T/.test(matchingEntry.timestamp) : false,"
        "  render_did_not_throw_to_caller: !render_threw,"
        "  matching_entry_excerpt: matchingEntry ? matchingEntry.error.slice(0, 400) : null,"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "logError_called_when_boundary_triggers", "ok": False,
                "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "logError_called_when_boundary_triggers", "ok": False,
                "raw_stdout": r["stdout"][:300]}
    return {
        "name": "logError_called_when_boundary_triggers",
        "ok": (
            parsed.get("delta_count", 0) >= 1
            and parsed.get("unique_msg_logged") is True
            and parsed.get("has_boundary_label_prefix") is True
            and parsed.get("has_unique_throw_msg") is True
            and parsed.get("has_stack_trace") is True
            and parsed.get("has_component_stack") is True
            and parsed.get("has_iso_timestamp") is True
        ),
        **parsed,
    }


def case_logError_NOT_called_when_no_throw() -> dict:
    """正常 render 不应增加 error count（防假阳性：测试可能因其他原因 +1）。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import * as React from 'react';"
        "import { render, Text } from 'ink';"
        "import { Writable } from 'node:stream';"
        "import { MossenErrorBoundary } from './components/MossenErrorBoundary.tsx';"
        "import { getInMemoryErrors } from './utils/log.ts';"
        ""
        "const errorsBefore = getInMemoryErrors().length;"
        ""
        "function Healthy() { return React.createElement(Text, null, 'healthy'); }"
        ""
        "const chunks: Buffer[] = [];"
        "const stdout = new Writable({write(chunk, _e, cb) { chunks.push(Buffer.from(chunk)); cb(); }});"
        "(stdout as any).isTTY = true;"
        "(stdout as any).columns = 80;"
        "(stdout as any).rows = 24;"
        ""
        "const inst: any = render(React.createElement(MossenErrorBoundary, {label: 'GAP1Healthy'}, React.createElement(Healthy)), {stdout: stdout as any, exitOnCtrlC: false, patchConsole: false});"
        "await new Promise(r => setTimeout(r, 200));"
        "if (inst && typeof inst.unmount === 'function') { try { inst.unmount(); } catch {} }"
        ""
        "const errorsAfter = getInMemoryErrors().length;"
        "process.stdout.write(JSON.stringify({"
        "  errors_before: errorsBefore,"
        "  errors_after: errorsAfter,"
        "  delta_count: errorsAfter - errorsBefore,"
        "  no_log_added: errorsAfter === errorsBefore,"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "logError_NOT_called_when_no_throw", "ok": False,
                "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "logError_NOT_called_when_no_throw", "ok": False,
                "raw_stdout": r["stdout"][:300]}
    return {
        "name": "logError_NOT_called_when_no_throw",
        "ok": parsed.get("no_log_added") is True,
        **parsed,
    }


def main() -> int:
    results = [
        case_logError_called_when_boundary_triggers(),
        case_logError_NOT_called_when_no_throw(),
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
