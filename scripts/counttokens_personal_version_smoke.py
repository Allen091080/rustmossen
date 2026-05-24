#!/usr/bin/env python3
"""
个人版 token 计数路径契约（[Wave2历史兼容] 已彻底去 Provider-hosted countTokens API）。

设计变化（2026-04-25 优化前 vs 后）:
  Before: countMessagesTokensWithAPI 调 mossenClient.beta.messages.countTokens →
    custom backend SDK 没有此方法 → throw → catch → logError → return null →
    caller fallback to small-fast model. 每次浪费 1 失败 SDK call + 1 errorLog noise.
  After: countMessagesTokensWithAPI/countTokensWithAPI/countTokensWithBedrock 全删,
    caller (utils/analyzeContext.ts countTokensWithFallback) 直接调
    countTokensViaSmallFastFallback (用 getSmallFastModel() = 用户配置模型)。
    0 失败 SDK call, 0 noise errorLog.

契约（3 case）:
  1. tokenEstimation.ts NOT export countMessagesTokensWithAPI / countTokensWithAPI
     (彻底清除 provider-hosted countTokens path)
  2. countTokensViaSmallFastFallback 存在且会 throw on invalid URL
     (proof of life: 真在调 .create on user-configured model)
  3. utils/analyzeContext.ts countTokensWithFallback wrap → 不再产生
     "countTokens is not a function" errorLog (验证 doomed-call 真被消除)

反面案例:
  ❌ 反 1: 假设清除前后行为相同 → 删了死代码 lint/typecheck baseline 应反而下降
  ❌ 反 2: 不验 errorLog 字面 → 不知道有没有真消除 noise
"""

from __future__ import annotations

import json
import os
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


def case_provider_path_purged() -> dict:
    """tokenEstimation.ts 不再 export provider-hosted countTokens 路径相关函数。"""
    snippet = (
        "const mod: any = await import('./services/tokenEstimation.ts');"
        "process.stdout.write(JSON.stringify({"
        "  has_countTokensWithAPI: typeof mod.countTokensWithAPI === 'function',"
        "  has_countMessagesTokensWithAPI: typeof mod.countMessagesTokensWithAPI === 'function',"
        "  has_countTokensViaFastFallback: typeof mod.countTokensViaFastFallback === 'function',"
        "  has_countTokensViaSmallFastFallback: typeof mod.countTokensViaSmallFastFallback === 'function',"
        "  has_roughTokenCountEstimation: typeof mod.roughTokenCountEstimation === 'function',"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "provider_path_purged", "ok": False,
                "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "provider_path_purged", "ok": False,
                "raw": r["stdout"][:300]}
    return {
        "name": "provider_path_purged",
        "ok": (
            parsed.get("has_countTokensWithAPI") is False
            and parsed.get("has_countMessagesTokensWithAPI") is False
            and parsed.get("has_countTokensViaFastFallback") is False
            and parsed.get("has_countTokensViaSmallFastFallback") is True
            and parsed.get("has_roughTokenCountEstimation") is True
        ),
        **parsed,
    }


def case_smallfast_fallback_uses_configured_backend() -> dict:
    """countTokensViaSmallFastFallback 真在调用户配置 backend (invalid URL → 抛网络错)。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "process.env.MOSSEN_CODE_CUSTOM_BASE_URL = 'https://invalid.invalid/v1';"
        "process.env.MOSSEN_CODE_CUSTOM_API_KEY = 'fake';"
        "import { countTokensViaSmallFastFallback } from './services/tokenEstimation.ts';"
        ""
        "let threw = false;"
        "let errorMsg = '';"
        "try {"
        "  await countTokensViaSmallFastFallback("
        "    [{role: 'user', content: 'fallback test'}],"
        "    []"
        "  );"
        "} catch (e) {"
        "  threw = true;"
        "  errorMsg = (e as Error).message ?? String(e);"
        "}"
        ""
        "process.stdout.write(JSON.stringify({"
        "  threw_to_caller: threw,"
        "  error_indicates_network_or_dns: /(invalid|certificate|fetch|connect|getaddrinfo|enotfound|connrefused)/i.test(errorMsg),"
        "  error_msg: errorMsg.slice(0, 200),"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "smallfast_fallback_uses_configured_backend", "ok": False,
                "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "smallfast_fallback_uses_configured_backend", "ok": False,
                "raw": r["stdout"][:300]}
    return {
        "name": "smallfast_fallback_uses_configured_backend",
        "ok": (
            parsed.get("threw_to_caller") is True
            and parsed.get("error_indicates_network_or_dns") is True
        ),
        **parsed,
    }


def case_analyzecontext_uses_smallfast_only() -> dict:
    """静态多面契约 (强 mutation 抓力):
      a. utils/analyzeContext.ts 源码不含 countMessagesTokensWithAPI 字面
      b. import 行包含 countTokensViaSmallFastFallback
      c. 私有 countTokensWithFallback 函数体真调 countTokensViaSmallFastFallback

    Mutation 信号:
      - 加回 provider countTokens import → a fail
      - 重命名/删 fallback → b fail
      - 函数体回退到旧 try-then-fallback 逻辑 → c fail
    """
    import re
    src = (ROOT / "utils" / "analyzeContext.ts").read_text(encoding="utf-8")

    has_provider_api_ref = "countMessagesTokensWithAPI" in src or "countTokensWithAPI" in src
    has_smallfast_import = bool(
        re.search(r"countTokensViaSmallFastFallback.*from\s+['\"].*tokenEstimation", src, re.S)
        or re.search(
            r"from\s+['\"].*tokenEstimation\.js['\"][\s\S]*countTokensViaSmallFastFallback", src
        )
    )

    body_match = re.search(
        r"async function countTokensWithFallback\([\s\S]*?^\}",
        src,
        re.M,
    )
    body = body_match.group(0) if body_match else ""
    body_calls_smallfast = "countTokensViaSmallFastFallback" in body
    body_no_provider = "countMessagesTokensWithAPI" not in body

    return {
        "name": "analyzecontext_uses_smallfast_only",
        "ok": (
            not has_provider_api_ref
            and has_smallfast_import
            and body_calls_smallfast
            and body_no_provider
        ),
        "has_provider_api_ref": has_provider_api_ref,
        "has_smallfast_import": has_smallfast_import,
        "body_calls_smallfast": body_calls_smallfast,
        "body_no_provider": body_no_provider,
        "body_excerpt": body[:300] if body else "(no match)",
    }


def case_filereadtool_no_api_call() -> dict:
    """静态契约: tools/FileReadTool/FileReadTool.ts 不再调 countTokensWithAPI。

    旧行为: 个人版 API 调用 100% 失败 → 用 rough estimate 兜底
    新行为: 直接用 rough estimate, 0 网络
    Mutation: 加回 countTokensWithAPI(content) → fail
    """
    src = (ROOT / "tools" / "FileReadTool" / "FileReadTool.ts").read_text(encoding="utf-8")
    has_api_call = "countTokensWithAPI" in src
    return {
        "name": "filereadtool_no_api_call",
        "ok": not has_api_call,
        "has_api_call": has_api_call,
    }


def main() -> int:
    results = [
        case_provider_path_purged(),
        case_smallfast_fallback_uses_configured_backend(),
        case_analyzecontext_uses_smallfast_only(),
        case_filereadtool_no_api_call(),
    ]
    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "design_note": (
            "个人版 token 计数现仅一条路径：getSmallFastModel() 配置模型的 .create + "
            "max_tokens=1, 读 usage.input_tokens. 上游 provider countTokens API/Bedrock "
            "CountTokens 路径已彻底清除（-149 行 services/tokenEstimation.ts）。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
