#!/usr/bin/env python3
"""
GAP 4: 真测 mossen API client (services/api/client.ts) 错误路径。

⚠️ 之前的 L8.3 只用 raw fetch + getCustomBackendBaseUrl 凑合，
不是真测 mossen 的 API client wrapper。本 smoke 真 import getMossenClient
并通过其返回的 SDK 实例发请求。

契约（5 条 falsifiable）：
  1. import getMossenClient 成功（mossen 真模块）
  2. getMossenClient({apiKey, maxRetries:0}) 返回 SDK 实例（不 crash on construction）
  3. SDK 实例有 messages.create 方法（MossenSDK shape）
  4. 设无效 baseUrl 后 messages.create 调用 reject (不 hang)
  5. 捕获的 error 含具体网络/连接错误标识（不是 generic Error）

反面案例：
  ❌ 反 1: 只 import getMossenClient 不 instantiate → 不证 builder 有错误处理
  ❌ 反 2: 测 raw fetch 而不是 SDK 路径 → 之前已经犯过的错
  ❌ 反 3: 只 catch any throw 不验 error 内容 → SDK 可能吞错误返回 generic 提示

User path:
  Mossen 主 loop → getMossenClient(...) → client.messages.create(...) → 网络
  本测试覆盖 client builder + 1 个 method invocation 的错误传播

Import 自检:
  ✅ from services/api/client (mossen 真 client builder)
  ✅ from utils/customBackend (mossen 配置)
  ❌ NOT raw fetch (上次的偷工)
"""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")


def _bun(snippet: str, env: dict[str, str] | None = None, timeout: int = 60) -> dict:
    full_env = os.environ.copy()
    if env:
        full_env.update(env)
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
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return None


def case_getMossenClient_constructs_with_bad_url() -> dict:
    """getMossenClient 在配了无效 baseUrl 时仍能成功构造（不 crash）。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "process.env.MOSSEN_CODE_CUSTOM_BASE_URL = 'https://invalid-host-test-mossen-gap4.invalid/v1';"
        "process.env.MOSSEN_CODE_CUSTOM_API_KEY = 'fake-test-key-gap4';"
        "import { getMossenClient } from './services/api/client.ts';"
        "let constructed = false;"
        "let constructError: string | null = null;"
        "let hasMessagesCreate = false;"
        "try {"
        "  const client = await getMossenClient({apiKey: 'fake-test-key-gap4', maxRetries: 0, source: 'gap4-test'});"
        "  constructed = client !== null && client !== undefined;"
        "  hasMessagesCreate = constructed && typeof (client as any).beta?.messages?.create === 'function';"
        "} catch (e) {"
        "  constructError = (e as Error).message ?? String(e);"
        "}"
        "process.stdout.write(JSON.stringify({"
        "  imported_getMossenClient: typeof getMossenClient === 'function',"
        "  constructed,"
        "  has_messages_create_method: hasMessagesCreate,"
        "  constructError,"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {
            "name": "getMossenClient_constructs_with_bad_url",
            "ok": False,
            "stderr": r["stderr"][:500],
            "raw_stdout": r["stdout"][:300],
        }
    return {
        "name": "getMossenClient_constructs_with_bad_url",
        "ok": (
            parsed.get("imported_getMossenClient") is True
            and parsed.get("constructed") is True
            and parsed.get("has_messages_create_method") is True
        ),
        **parsed,
    }


def case_messages_create_rejects_with_bad_url() -> dict:
    """SDK 实例 messages.create 调用在无效 baseUrl 时 reject (网络错误)，不 hang。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "process.env.MOSSEN_CODE_CUSTOM_BASE_URL = 'https://invalid-host-test-mossen-gap4.invalid/v1';"
        "process.env.MOSSEN_CODE_CUSTOM_API_KEY = 'fake-test-key-gap4';"
        "import { getMossenClient } from './services/api/client.ts';"
        "const client: any = await getMossenClient({apiKey: 'fake-test-key-gap4', maxRetries: 0, source: 'gap4-test'});"
        "let rejected = false;"
        "let rejectError: string = '';"
        "let timed_out = false;"
        "const timer = setTimeout(() => { timed_out = true; }, 8000);"
        "try {"
        "  await client.beta.messages.create({"
        "    model: 'example-large',"
        "    max_tokens: 10,"
        "    messages: [{role: 'user', content: 'test'}],"
        "  });"
        "  clearTimeout(timer);"
        "} catch (e) {"
        "  clearTimeout(timer);"
        "  rejected = true;"
        "  rejectError = (e as Error).message ?? String(e);"
        "}"
        "process.stdout.write(JSON.stringify({"
        "  call_rejected: rejected,"
        "  did_not_hang: !timed_out,"
        "  error_indicates_network: /(invalid|getaddrinfo|ENOTFOUND|certificate|fetch failed|socket|closed|connect|connection|unable)/i.test(rejectError),"
        "  error_excerpt: rejectError.slice(0, 200),"
        "  error_is_substantive: rejectError.length > 5,"
        "}) + '\\n');"
    )
    r = _bun(snippet, timeout=30)
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {
            "name": "messages_create_rejects_with_bad_url",
            "ok": False,
            "stderr": r["stderr"][:500],
            "raw_stdout": r["stdout"][:300],
        }
    return {
        "name": "messages_create_rejects_with_bad_url",
        "ok": (
            parsed.get("call_rejected") is True
            and parsed.get("did_not_hang") is True
            and parsed.get("error_indicates_network") is True
            and parsed.get("error_is_substantive") is True
        ),
        **parsed,
    }


def case_sdk_shape_for_custom_backend() -> dict:
    """断言 personal 版 custom backend 的 SDK shape：beta.messages 只暴露 create。
    如果 SDK 加了新方法 (countTokens / models / etc)，本断言 fail 强制 re-eval
    GAP4 是否需要扩展测试覆盖。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "process.env.MOSSEN_CODE_CUSTOM_BASE_URL = 'https://invalid.invalid/v1';"
        "process.env.MOSSEN_CODE_CUSTOM_API_KEY = 'fake';"
        "import { getMossenClient } from './services/api/client.ts';"
        "const client: any = await getMossenClient({apiKey: 'fake', maxRetries: 0});"
        "const topKeys = Object.keys(client);"
        "const betaKeys = client.beta ? Object.keys(client.beta) : [];"
        "const messagesProto = client.beta?.messages ? Object.getOwnPropertyNames(Object.getPrototypeOf(client.beta.messages)) : [];"
        "const messagesOwn = client.beta?.messages ? Object.keys(client.beta.messages) : [];"
        "const messagesAll = [...new Set([...messagesProto, ...messagesOwn])].filter((k: any) => k !== 'constructor');"
        "process.stdout.write(JSON.stringify({"
        "  top_level_keys: topKeys,"
        "  beta_keys: betaKeys,"
        "  messages_all_methods: messagesAll,"
        "  has_create: typeof client.beta?.messages?.create === 'function',"
        "  has_countTokens: typeof client.beta?.messages?.countTokens === 'function',"
        "  has_models: typeof client.models === 'object' || typeof client.beta?.models === 'object',"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "sdk_shape_custom_backend", "ok": False,
                "stderr": r["stderr"][:500]}
    # 断言只有 create，无其他 SDK API methods（personal 版 custom backend 真行为）
    # 注：messages_all_methods 含 Object.prototype 默认方法（toString 等），不是 SDK API
    only_create = (
        parsed.get("has_create") is True
        and parsed.get("has_countTokens") is False
        and parsed.get("has_models") is False
        and parsed.get("top_level_keys") == ["beta"]
        and parsed.get("beta_keys") == ["messages"]
    )
    return {
        "name": "sdk_shape_custom_backend_exposes_only_create",
        "ok": only_create,
        "design_note": "Personal 版 custom backend 只暴露 create；其他方法（countTokens 等）"
                        "可能在 Bedrock/Vertex/Foundry 后端有，但 personal 默认走 custom",
        **parsed,
    }


def main() -> int:
    results = [
        case_getMossenClient_constructs_with_bad_url(),
        case_messages_create_rejects_with_bad_url(),
        case_sdk_shape_for_custom_backend(),
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
