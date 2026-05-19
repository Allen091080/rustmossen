#!/usr/bin/env python3
"""
mcpContentNeedsTruncation 故障安全契约 (修复 anthropic-countTokens 清理后引入的 bug)。

Bug 历史:
  原代码: catch error → return false (假定不需要截断)
    问题: 个人版 SDK 没 countTokens → 永远 throw → 永远 return false →
          超长 MCP 输出永远不被截断 → 内存可能爆。
  修复 (commit 8ddf54e):
    catch → fall through to contentSizeEstimate > cap check
    fault-safe direction: 网络/SDK 异常时仍然按内容大小阈值兜底截断。

契约 (4 case):
  1. tiny_content_no_truncate: 小内容直接 return false (早出, 不调任何 API)
  2. large_content_with_broken_backend: 大内容 + 失败 backend → return TRUE
     (这是 bug 修复的核心 — 原代码这种情况 return false, 现在返回 true)
  3. static_failsafe_present: 源码静态契约 catch 块后必须有 contentSizeEstimate
     兜底 return (字面 grep, mutation 信号)
  4. static_no_unconditional_false: catch 块不能直接 return false
     (mutation 信号: bug 回退会被抓)

反测信号 (mutation):
  - case 2: 如果 catch 改回 'return false' → 大内容 + 失败 backend 返回 false → fail
  - case 3: 如果 contentSizeEstimate 兜底删掉 → fail
  - case 4: 如果 catch 后写 'return false' → fail
"""

from __future__ import annotations

import json
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")


def _bun(snippet: str, env_override: dict | None = None, timeout: int = 60) -> dict:
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


def case_tiny_content_no_truncate() -> dict:
    """小内容: getContentSizeEstimate <= cap * 0.5 → return false 早出。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { mcpContentNeedsTruncation } from './utils/mcpValidation.ts';"
        ""
        "const result = await mcpContentNeedsTruncation('hello world');"
        "process.stdout.write(JSON.stringify({"
        "  needs_truncation: result,"
        "  expected: false,"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "tiny_content_no_truncate", "ok": False,
                "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "tiny_content_no_truncate", "ok": False,
                "raw": r["stdout"][:300]}
    return {
        "name": "tiny_content_no_truncate",
        "ok": parsed.get("needs_truncation") is False,
        **parsed,
    }


def case_large_content_with_broken_backend() -> dict:
    """大内容 + invalid backend → fallback 抛错 → fail-safe 兜底 return TRUE。

    这是 bug 修复的核心证明: 原代码这种情况 return false (静默不截断)。
    内容设计:
      - rough estimate (chars/4) > getMaxMcpOutputTokens() (default 25000)
      - 即 chars > 100_000
    """
    big_content_chars = 200_000
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "process.env.MOSSEN_CODE_CUSTOM_BASE_URL = 'https://invalid.invalid/v1';"
        "process.env.MOSSEN_CODE_CUSTOM_API_KEY = 'fake';"
        "import { mcpContentNeedsTruncation, getMaxMcpOutputTokens, getContentSizeEstimate } from './utils/mcpValidation.ts';"
        ""
        f"const big = 'x'.repeat({big_content_chars});"
        "const cap = getMaxMcpOutputTokens();"
        "const estimate = getContentSizeEstimate(big);"
        "const result = await mcpContentNeedsTruncation(big);"
        "process.stdout.write(JSON.stringify({"
        "  cap_tokens: cap,"
        "  content_size_estimate: estimate,"
        "  estimate_exceeds_cap: estimate > cap,"
        "  needs_truncation: result,"
        "  expected: true,"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "large_content_with_broken_backend", "ok": False,
                "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "large_content_with_broken_backend", "ok": False,
                "raw": r["stdout"][:300]}
    return {
        "name": "large_content_with_broken_backend",
        "ok": (
            parsed.get("needs_truncation") is True
            and parsed.get("estimate_exceeds_cap") is True
        ),
        **parsed,
    }


def case_static_failsafe_present() -> dict:
    """源码静态契约: mcpContentNeedsTruncation 函数体在 try/catch 之后,
    最末必须有 'contentSizeEstimate' + 'getMaxMcpOutputTokens' 比较 (fail-safe)。
    """
    src = (ROOT / "utils" / "mcpValidation.ts").read_text(encoding="utf-8")

    fn_match = re.search(
        r"export async function mcpContentNeedsTruncation\([\s\S]*?^\}",
        src,
        re.M,
    )
    body = fn_match.group(0) if fn_match else ""

    has_failsafe = (
        "contentSizeEstimate > getMaxMcpOutputTokens()" in body
        or "contentSizeEstimate >= getMaxMcpOutputTokens()" in body
        or re.search(r"contentSizeEstimate\s*>\s*getMaxMcpOutputTokens", body) is not None
    )
    has_fallback_call = "countTokensViaSmallFastFallback" in body
    no_anthropic_call = "countMessagesTokensWithAPI" not in body

    return {
        "name": "static_failsafe_present",
        "ok": has_failsafe and has_fallback_call and no_anthropic_call,
        "has_failsafe_size_check": has_failsafe,
        "has_smallfast_fallback_call": has_fallback_call,
        "no_anthropic_call": no_anthropic_call,
        "body_excerpt": body[:400] if body else "(no match)",
    }


def case_static_catch_no_unconditional_false() -> dict:
    """源码静态契约: catch 块不能直接 'return false' (会重新引入 bug)。

    Mutation 信号: 如果 catch 后写 'return false', 立即 fail。
    允许形式: catch { logError(error) } 然后 fall-through 到下面的 return。
    """
    src = (ROOT / "utils" / "mcpValidation.ts").read_text(encoding="utf-8")
    fn_match = re.search(
        r"export async function mcpContentNeedsTruncation\([\s\S]*?^\}",
        src,
        re.M,
    )
    body = fn_match.group(0) if fn_match else ""
    catch_match = re.search(r"catch\s*\([^)]*\)\s*\{[\s\S]*?\}", body)
    catch_body = catch_match.group(0) if catch_match else ""

    bad_return_false_in_catch = bool(re.search(r"return\s+false", catch_body))

    return {
        "name": "static_catch_no_unconditional_false",
        "ok": not bad_return_false_in_catch,
        "bad_return_false_in_catch": bad_return_false_in_catch,
        "catch_excerpt": catch_body[:200] if catch_body else "(no match)",
    }


def main() -> int:
    results = [
        case_tiny_content_no_truncate(),
        case_large_content_with_broken_backend(),
        case_static_failsafe_present(),
        case_static_catch_no_unconditional_false(),
    ]
    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "design_note": (
            "原 mcpValidation catch 后 return false → 个人版 SDK 失败时永远不截断 "
            "(故障安全反向). 修复后 catch fall-through 到 contentSize > cap 兜底, "
            "失败时仍按大小判定是否截断 (commit 8ddf54e 引入)."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
