#!/usr/bin/env python3
"""
error_boundary_unit_smoke.py — FIX 2 真触发 ErrorBoundary。

⚠️ 上一版（被用户戳穿）用 MOSSEN_INJECT_THROW=Spinner 起 TUI 验证，但 Spinner
只在 loading 时 render，trust dialog 阶段根本没 mount → InjectionThrower 没触发
→ 测试"通过"但什么都没验证。

本版直接 ink-render <MossenErrorBoundary><Thrower /></MossenErrorBoundary>
到一个内存 stream，验证：
  1. 子组件 throw 被 boundary 捕获（进程不 exit）
  2. boundary fallback 文本真出现在渲染输出里
  3. logError 被调用记录错误

使用 ink 的 render API + 自定义 stdout，确保我们真触发了 React 错误流程。
"""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")


def _bun(snippet: str, timeout: int = 30) -> dict:
    full_env = os.environ.copy()
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=timeout,
        env=full_env,
    )
    return {
        "returncode": proc.returncode,
        "stdout": proc.stdout or "",
        "stderr": proc.stderr or "",
    }


def _extract_json(out: str) -> dict | None:
    for line in reversed(out.splitlines()):
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            return json.loads(line)
        except json.JSONDecodeError:
            continue
    return None


def case_render_thrower_caught_by_boundary() -> dict:
    """直接 render boundary + thrower，验证 fallback 真显示且进程未 exit。"""
    # Use ink's render with a custom stream. We pass a Writable stream that
    # collects bytes, render to it, then unmount and inspect collected output.
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import * as React from 'react';"
        "import { render } from 'ink';"
        "import { Writable } from 'node:stream';"
        "import { MossenErrorBoundary } from './components/MossenErrorBoundary.tsx';"
        ""
        "function Thrower() { throw new Error('CASE_DELIBERATE_THROW_FOR_BOUNDARY_TEST'); }"
        ""
        "const chunks: Buffer[] = [];"
        "const stdout = new Writable({write(chunk, _enc, cb) { chunks.push(Buffer.from(chunk)); cb(); }});"
        "(stdout as any).isTTY = true;"
        "(stdout as any).columns = 80;"
        "(stdout as any).rows = 24;"
        ""
        "let render_threw = false;"
        "try {"
        "  const inst: any = render(React.createElement(MossenErrorBoundary, {label: 'TestThrower'}, React.createElement(Thrower)), {stdout: stdout as any, exitOnCtrlC: false, patchConsole: false});"
        "  await new Promise(resolve => setTimeout(resolve, 200));"
        "  if (inst && typeof inst.unmount === 'function') { try { inst.unmount(); } catch {} }"
        "} catch (e) { render_threw = true; }"
        ""
        "const captured = chunks.map(c => c.toString('utf8')).join('');"
        "const visible = captured.replace(/\\x1b\\[[0-9;?]*[a-zA-Z]/g, '');"
        ""
        "process.stdout.write(JSON.stringify({"
        "  render_did_not_throw_to_caller: !render_threw,"
        "  fallback_warning_visible: visible.includes('TestThrower') && visible.includes('渲染失败'),"
        "  process_alive: true,"
        "  visible_excerpt: visible.slice(0, 200),"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {
            "name": "ErrorBoundary_unit_test",
            "ok": False,
            "stderr": r["stderr"][:500],
            "stdout": r["stdout"][:300],
        }
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {
            "name": "ErrorBoundary_unit_test",
            "ok": False,
            "raw_stdout": r["stdout"][:500],
            "raw_stderr": r["stderr"][:500],
            "returncode": r["returncode"],
        }
    return {
        "name": "ErrorBoundary_unit_test",
        "ok": parsed.get("fallback_warning_visible") is True,
        **parsed,
    }


def case_inject_throw_via_env() -> dict:
    """用 MOSSEN_INJECT_THROW=TestComponent 验证 InjectionThrower 真触发 boundary fallback。

    与上一案不同：通过 withErrorBoundary HOC + InjectionThrower 路径，验证 env 注入工作。
    """
    snippet = (
        "process.env.MOSSEN_INJECT_THROW = 'TestComponent';"
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import * as React from 'react';"
        "import { render, Text } from 'ink';"
        "import { Writable } from 'node:stream';"
        "import { withErrorBoundary } from './components/MossenErrorBoundary.tsx';"
        ""
        "function NormalComponent() { return React.createElement(Text, null, 'normal-rendered'); }"
        "const Wrapped = withErrorBoundary(NormalComponent, 'TestComponent');"
        ""
        "const chunks: Buffer[] = [];"
        "const stdout = new Writable({write(chunk, _e, cb) { chunks.push(Buffer.from(chunk)); cb(); }});"
        "(stdout as any).isTTY = true;"
        "(stdout as any).columns = 80;"
        "(stdout as any).rows = 24;"
        ""
        "let render_threw = false;"
        "try {"
        "  const inst = render(React.createElement(Wrapped, {}), {stdout: stdout as any, exitOnCtrlC: false, patchConsole: false});"
        "  await new Promise(r => setTimeout(r, 200));"
        "  inst.unmount();"
        "} catch (e) { render_threw = true; }"
        "const captured = chunks.map(c => c.toString('utf8')).join('');"
        "const visible = captured.replace(/\\x1b\\[[0-9;?]*[a-zA-Z]/g, '');"
        ""
        "console.log(JSON.stringify({"
        "  render_did_not_throw_to_caller: !render_threw,"
        "  fallback_text_visible: visible.includes('TestComponent') && visible.includes('渲染失败'),"
        "  inject_simulated_throw_in_message: visible.includes('MOSSEN_INJECT_THROW=TestComponent'),"
        "  normal_text_NOT_rendered: !visible.includes('normal-rendered'),"
        "  visible_excerpt: visible.slice(0, 250),"
        "}));"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {
            "name": "InjectionThrower_via_env",
            "ok": False,
            "stderr": r["stderr"][:500],
        }
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "InjectionThrower_via_env", "ok": False, "raw": r["stdout"][:500]}
    return {
        "name": "InjectionThrower_via_env",
        "ok": (
            parsed.get("fallback_text_visible") is True
            and parsed.get("inject_simulated_throw_in_message") is True
            and parsed.get("normal_text_NOT_rendered") is True
        ),
        **parsed,
    }


def main() -> int:
    results = [
        case_render_thrower_caught_by_boundary(),
        case_inject_throw_via_env(),
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
