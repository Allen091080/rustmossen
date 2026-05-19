#!/usr/bin/env python3
"""
GAP 2: 测 MossenErrorBoundary 的 silent + 自定义 fallback prop。

契约（5 条 falsifiable）：
  1. silent={true} + boundary 触发 → 输出 NOT 包含默认 ⚠
  2. silent={true} + boundary 触发 → render 返回 null（输出近空）
  3. fallback={fn} + boundary 触发 → 输出包含 fn 返回值标记
  4. fallback({error}) 接收的 error.message 是真 throw 内容
  5. fallback({label}) 接收的 label 是 boundary 的 label

反面案例：
  ❌ 反 1: 只测 prop type 编译通过 — 不证明 runtime 用
  ❌ 反 2: 测 silent 不验证默认 fallback 真没渲染（只验证不崩）

User path:
  Boundary state error → render() 检查 props.silent → 返回 null
                       → 检查 props.fallback → 调用并返回结果
                       → 都没 → 返回默认 ⚠ fallback

Mutation point: 注释 `if (this.props.silent) return null` →
  silent 测试 fail（输出仍含默认 ⚠）
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


_RENDER_HARNESS = (
    "import { enableConfigs } from './utils/config.ts';"
    "enableConfigs();"
    "import * as React from 'react';"
    "import { render, Text } from 'ink';"
    "import { Writable } from 'node:stream';"
    "import { MossenErrorBoundary } from './components/MossenErrorBoundary.tsx';"
    ""
    "function makeStdout() {"
    "  const chunks: Buffer[] = [];"
    "  const stdout = new Writable({write(chunk, _e, cb) { chunks.push(Buffer.from(chunk)); cb(); }});"
    "  (stdout as any).isTTY = true;"
    "  (stdout as any).columns = 80;"
    "  (stdout as any).rows = 24;"
    "  return {stdout, chunks};"
    "}"
    ""
    "async function renderAndCapture(node: any): Promise<string> {"
    "  const {stdout, chunks} = makeStdout();"
    "  const inst: any = render(node, {stdout: stdout as any, exitOnCtrlC: false, patchConsole: false});"
    "  await new Promise(r => setTimeout(r, 200));"
    "  if (inst && typeof inst.unmount === 'function') { try { inst.unmount(); } catch {} }"
    "  const captured = chunks.map(c => c.toString('utf8')).join('');"
    "  return captured.replace(/\\x1b\\[[0-9;?]*[a-zA-Z]/g, '');"
    "}"
)


def case_silent_suppresses_default_fallback() -> dict:
    """silent={true} → 默认 ⚠ fallback 不渲染，但 logError 仍调用。"""
    snippet = (
        _RENDER_HARNESS +
        "import { getInMemoryErrors } from './utils/log.ts';"
        "const errorsBefore = getInMemoryErrors().length;"
        "function Thrower() { throw new Error('SILENT_TEST_THROW_99'); }"
        "const visible = await renderAndCapture("
        "  React.createElement(MossenErrorBoundary, {label: 'SilentTest', silent: true},"
        "    React.createElement(Thrower)"
        "  )"
        ");"
        "const errorsAfter = getInMemoryErrors();"
        "const newEntry = errorsAfter.slice(errorsBefore).find((e: any) => e.error.includes('SILENT_TEST_THROW_99'));"
        "process.stdout.write(JSON.stringify({"
        "  has_default_warning: visible.includes('⚠'),"
        "  has_label: visible.includes('SilentTest'),"
        "  has_throw_msg: visible.includes('SILENT_TEST_THROW_99'),"
        "  visible_excerpt: visible.slice(0, 200),"
        "  visible_length: visible.length,"
        "  logError_still_called_when_silent: newEntry !== undefined,"
        "  logged_has_label_prefix: newEntry ? newEntry.error.includes('[MossenErrorBoundary:SilentTest]') : false,"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "silent_suppresses_default_fallback", "ok": False,
                "stderr": r["stderr"][:500], "raw": r["stdout"][:300]}
    return {
        "name": "silent_suppresses_default_fallback",
        "ok": (
            parsed.get("has_default_warning") is False
            and parsed.get("has_label") is False
            and parsed.get("has_throw_msg") is False
            and parsed.get("logError_still_called_when_silent") is True
            and parsed.get("logged_has_label_prefix") is True
        ),
        **parsed,
    }


def case_custom_fallback_replaces_default() -> dict:
    """fallback={fn} → 默认 ⚠ 不渲染，fn 输出渲染。"""
    snippet = (
        _RENDER_HARNESS +
        "function Thrower() { throw new Error('CUSTOM_FB_THROW_MSG_42'); }"
        "const visible = await renderAndCapture("
        "  React.createElement(MossenErrorBoundary, {"
        "    label: 'CustomFbLabel',"
        "    fallback: (err: Error, label: string | undefined) =>"
        "      React.createElement(Text, null, 'CUSTOM_FB_OUTPUT[' + label + '][' + err.message + ']')"
        "  }, React.createElement(Thrower))"
        ");"
        "process.stdout.write(JSON.stringify({"
        "  custom_marker_visible: visible.includes('CUSTOM_FB_OUTPUT'),"
        "  custom_label_passed: visible.includes('[CustomFbLabel]'),"
        "  custom_error_passed: visible.includes('[CUSTOM_FB_THROW_MSG_42]'),"
        "  default_warning_NOT_rendered: !visible.includes('⚠'),"
        "  visible_excerpt: visible.slice(0, 200),"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "custom_fallback_replaces_default", "ok": False,
                "stderr": r["stderr"][:500], "raw": r["stdout"][:300]}
    return {
        "name": "custom_fallback_replaces_default",
        "ok": (
            parsed.get("custom_marker_visible") is True
            and parsed.get("custom_label_passed") is True
            and parsed.get("custom_error_passed") is True
            and parsed.get("default_warning_NOT_rendered") is True
        ),
        **parsed,
    }


def case_default_fallback_when_no_props() -> dict:
    """无 silent/fallback prop → 默认 ⚠ 渲染。（baseline，确保 silent/custom 是真 override）"""
    snippet = (
        _RENDER_HARNESS +
        "function Thrower() { throw new Error('DEFAULT_FB_TEST'); }"
        "const visible = await renderAndCapture("
        "  React.createElement(MossenErrorBoundary, {label: 'DefaultLabel'},"
        "    React.createElement(Thrower)"
        "  )"
        ");"
        "process.stdout.write(JSON.stringify({"
        "  has_default_warning: visible.includes('⚠'),"
        "  has_label: visible.includes('DefaultLabel'),"
        "  has_throw_msg: visible.includes('DEFAULT_FB_TEST'),"
        "  visible_excerpt: visible.slice(0, 200),"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "default_fallback_when_no_props", "ok": False,
                "stderr": r["stderr"][:500], "raw": r["stdout"][:300]}
    return {
        "name": "default_fallback_when_no_props",
        "ok": (
            parsed.get("has_default_warning") is True
            and parsed.get("has_label") is True
            and parsed.get("has_throw_msg") is True
        ),
        **parsed,
    }


def main() -> int:
    results = [
        case_default_fallback_when_no_props(),  # baseline first
        case_silent_suppresses_default_fallback(),
        case_custom_fallback_replaces_default(),
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
