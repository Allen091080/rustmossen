#!/usr/bin/env python3
"""
FIX-MORE-3: 测 errorQueue drain 路径 (sink attach 前 logError 的)。

调研：utils/log.ts:190-193 — logError 在 errorLogSink === null 时把 error
push 到 errorQueue。后续 attachErrorLogSink(sink) 时 drain queue 调用
sink.logError。

契约（3 case）：
  1. sink 未 attach + logError → in-memory log +1 (always added)
  2. sink 注册后 → spy 收到 drain 的 error (含 sink 注册前的)
  3. sink 注册后 logError → spy 直接收到 (不经 queue)

反面案例：
  ❌ 反 1: 测 attachErrorLogSink 多次 → 之后 logError 走 sink 不走 queue
  ❌ 反 2: 不验 spy 收到的 error 内容 — 可能 drain 调用空对象

User path:
  Mossen 启动序列：
    1. import 阶段早期 logError(...) → 走 queue
    2. setup() → initializeErrorLogSink() → drain queue → sink.logError(每条)
    3. 后续 logError → 直接 sink

Mutation point:
  改 utils/log.ts:117 的 errorQueue.length = 0 后跳过 drain 循环 →
  case 2 的 spy 不会收到先前 logged 的 error → fail
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


def case_logError_before_sink_then_drain() -> dict:
    """
    Sink attach 前 logError → errorQueue → attach spy sink → spy.logError 真被调。

    用 spy sink (不是真 errorLogSink) 因为：
    - 不需要 USER_TYPE=ant
    - 直接观察 sink.logError 调用次数 + 参数
    """
    # NOTE: 拼成单行后 // 注释会吞后续代码（已踩过坑两次），全部删掉
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { logError, getInMemoryErrors, attachErrorLogSink } from './utils/log.ts';"
        "const UNIQUE_BEFORE = 'GAP3FIX_BEFORE_' + Date.now();"
        "logError(new Error(UNIQUE_BEFORE));"
        "const memAfterBefore = getInMemoryErrors().length;"
        "const spyCalls: any[] = [];"
        "const spy = {"
        "  logError: (e: Error) => spyCalls.push({type: 'error', message: e.message}),"
        "  logMCPError: (s: string, e: any) => spyCalls.push({type: 'mcpError', server: s}),"
        "  logMCPDebug: (s: string, m: string) => spyCalls.push({type: 'mcpDebug', server: s}),"
        "  getErrorsPath: () => '/tmp/test',"
        "  getMCPLogsPath: (s: string) => '/tmp/test/mcp-' + s,"
        "};"
        "attachErrorLogSink(spy as any);"
        "const UNIQUE_AFTER = 'GAP3FIX_AFTER_' + Date.now();"
        "logError(new Error(UNIQUE_AFTER));"
        ""
        "process.stdout.write(JSON.stringify({"
        "  unique_before: UNIQUE_BEFORE,"
        "  unique_after: UNIQUE_AFTER,"
        "  in_memory_count: memAfterBefore,"
        "  spy_total_calls: spyCalls.length,"
        "  spy_received_before_marker: spyCalls.some(c => c.type === 'error' && c.message.includes(UNIQUE_BEFORE)),"
        "  spy_received_after_marker: spyCalls.some(c => c.type === 'error' && c.message.includes(UNIQUE_AFTER)),"
        "  spy_calls_summary: spyCalls.slice(0, 5).map(c => ({type: c.type, msg_excerpt: (c.message ?? '').slice(0, 60)})),"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "errorQueue_drain", "ok": False, "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "errorQueue_drain", "ok": False, "raw": r["stdout"][:300]}
    return {
        "name": "errorQueue_drain",
        "ok": (
            parsed.get("in_memory_count", 0) >= 1  # in-memory always logged
            and parsed.get("spy_total_calls", 0) >= 2  # before marker + after marker
            and parsed.get("spy_received_before_marker") is True  # drain真发生
            and parsed.get("spy_received_after_marker") is True  # 直接路径也工作
        ),
        **parsed,
    }


def case_attach_idempotent_no_double_drain() -> dict:
    """attachErrorLogSink 第二次调用是 no-op（不重置 sink, 不重新 drain）。

    防止 sink 被替换或 queue drain 多次的副作用。
    """
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { logError, attachErrorLogSink } from './utils/log.ts';"
        ""
        "const calls1: any[] = [];"
        "const sink1 = {"
        "  logError: (e: Error) => calls1.push(e.message),"
        "  logMCPError: () => {}, logMCPDebug: () => {},"
        "  getErrorsPath: () => '', getMCPLogsPath: () => '',"
        "};"
        "attachErrorLogSink(sink1 as any);"
        "const calls2: any[] = [];"
        "const sink2 = {"
        "  logError: (e: Error) => calls2.push(e.message),"
        "  logMCPError: () => {}, logMCPDebug: () => {},"
        "  getErrorsPath: () => '', getMCPLogsPath: () => '',"
        "};"
        "attachErrorLogSink(sink2 as any);"
        ""
        "logError(new Error('GAP3FIX_IDEMPOTENT_TEST'));"
        ""
        "process.stdout.write(JSON.stringify({"
        "  sink1_received: calls1.length,"
        "  sink2_received: calls2.length,"
        "  only_sink1_used: calls1.length === 1 && calls2.length === 0,"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "attach_is_idempotent", "ok": False, "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "attach_is_idempotent", "ok": False, "raw": r["stdout"][:300]}
    return {
        "name": "attach_is_idempotent",
        "ok": parsed.get("only_sink1_used") is True,
        **parsed,
    }


def main() -> int:
    results = [
        case_logError_before_sink_then_drain(),
        case_attach_idempotent_no_double_drain(),
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
