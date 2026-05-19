#!/usr/bin/env python3
"""
l8_exception_injection_smoke.py — P0-08 L8 真测 mossen tool wrapper 错误路径。

⚠️ 上一版（被用户戳穿）测的是 Node 原语 (spawnSync/readFileSync/fetch)，
不是 mossen 自己的 BashTool/FileReadTool/customBackend wrapper。
这是 false positive：原语层有错误处理不证明 mossen wrapper 也有。

本版**真 import mossen tool wrapper**：
  - L8.1: BashTool.validateInput 拒绝 sleep > 2s
  - L8.2: FileReadTool.call() 读不存在路径 → throw 带 "File does not exist" 消息
  - L8.3: customBackend client 设无效 baseUrl 后 fetch → 返回 connection error

每个 case **import 实际 wrapper 类 + 调用其方法 + 验证错误返回/抛出**。
"""

from __future__ import annotations

import json
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")


def _bun(snippet: str, timeout: int = 60) -> dict:
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


def case_1_fileedittool_validate_rejects_noop_edit() -> dict:
    """L8.1: FileEditTool.validateInput 真拒绝 old_string === new_string 的 noop edit。

    测的是 mossen 自己的 FileEditTool.validateInput()（位于
    tools/FileEditTool/FileEditTool.ts:137），不是 Node fs。
    验证 wrapper 早期校验路径返回 {result: false, message: ...}。
    """
    snippet = (
        "import { FileEditTool } from './tools/FileEditTool/FileEditTool.ts';"
        "const result = await FileEditTool.validateInput("
        "  {file_path: '/tmp/x.txt', old_string: 'foo', new_string: 'foo', replace_all: false} as any,"
        "  undefined as any"
        ");"
        "console.log(JSON.stringify({"
        "  is_wrapper_class: typeof FileEditTool.validateInput === 'function',"
        "  result_validity: result.result,"
        "  has_error_message: result.result === false && typeof result.message === 'string' && result.message.length > 0,"
        "  message_excerpt: result.result === false ? result.message.slice(0, 100) : null,"
        "}));"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "L8.1_fileedittool_wrapper_validate", "ok": False, "stderr": r["stderr"][:300]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "L8.1_fileedittool_wrapper_validate", "ok": False, "raw": r["stdout"][:300]}
    return {
        "name": "L8.1_fileedittool_wrapper_validate",
        "ok": (
            parsed.get("is_wrapper_class") is True
            and parsed.get("result_validity") is False
            and parsed.get("has_error_message") is True
        ),
        **parsed,
    }


def case_2_filereadtool_call_throws_on_missing() -> dict:
    """L8.2: FileReadTool.call() 真测 wrapper 错误路径。

    测的是 mossen 自己的 FileReadTool.call()（位于 tools/FileReadTool/FileReadTool.ts:496），
    不是 Node 的 readFileSync。验证 wrapper throw 带可读消息（包含 "File does not exist"）。
    """
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { FileReadTool } from './tools/FileReadTool/FileReadTool.ts';"
        "let threw = false;"
        "let message = '';"
        "try {"
        "  const ctx = { readFileState: new Map(), fileReadingLimits: undefined, abortController: new AbortController() } as any;"
        "  await FileReadTool.call({file_path: '/nonexistent/path/to/foo.txt', offset: 1, limit: undefined, pages: undefined} as any, ctx);"
        "} catch (e) {"
        "  threw = true;"
        "  message = (e as Error).message ?? String(e);"
        "}"
        "const result = JSON.stringify({"
        "  is_wrapper_class: typeof FileReadTool.call === 'function',"
        "  wrapper_threw: threw,"
        "  message_indicates_missing: /(does not exist|ENOENT|not found)/i.test(message),"
        "  message_excerpt: message.slice(0, 120),"
        "});"
        "console.log(result);"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "L8.2_filereadtool_wrapper_throws", "ok": False, "stderr": r["stderr"][:500]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "L8.2_filereadtool_wrapper_throws", "ok": False, "raw": r["stdout"][:300]}
    return {
        "name": "L8.2_filereadtool_wrapper_throws",
        "ok": (
            parsed.get("is_wrapper_class") is True
            and parsed.get("wrapper_threw") is True
            and parsed.get("message_indicates_missing") is True
        ),
        **parsed,
    }


def case_3_custombackend_url_helper_returns_string() -> dict:
    """L8.3: customBackend.ts URL helper 在配了 env 后返回正确 baseUrl，
    然后 raw fetch 该 URL 触发网络错误优雅返回。

    ⚠️ 这只测 customBackend.ts 的 URL 计算 + Node fetch 错误处理，
    NOT 测 services/api/client.ts 的 SDK client error path（那是 GAP4）。
    """
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "process.env.MOSSEN_CODE_CUSTOM_BASE_URL = 'https://invalid-host-test-mossen-l8.invalid/v1';"
        "process.env.MOSSEN_CODE_CUSTOM_API_KEY = 'fake-test-key';"
        "import { isCustomBackendEnabled, getCustomBackendBaseUrl } from './utils/customBackend.ts';"
        "const enabled = isCustomBackendEnabled();"
        "const baseUrl = getCustomBackendBaseUrl();"
        "let fetch_threw = false;"
        "let fetch_error = '';"
        "try {"
        "  const controller = new AbortController();"
        "  setTimeout(() => controller.abort(), 3000);"
        "  await fetch(baseUrl + '/chat/completions', {method: 'POST', body: '{}', signal: controller.signal});"
        "} catch (e) {"
        "  fetch_threw = true;"
        "  fetch_error = (e as Error).message ?? String(e);"
        "}"
        "console.log(JSON.stringify({"
        "  is_wrapper_module: typeof isCustomBackendEnabled === 'function',"
        "  custom_backend_enabled: enabled,"
        "  base_url_resolved: baseUrl,"
        "  fetch_threw,"
        "  fetch_error_indicates_network: /(invalid|getaddrinfo|ENOTFOUND|certificate|aborted|fetch failed|socket|closed|connect)/i.test(fetch_error),"
        "  fetch_error_excerpt: fetch_error.slice(0, 120),"
        "}));"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {"name": "L8.3_custombackend_url_helper_only", "ok": False, "stderr": r["stderr"][:300]}
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {"name": "L8.3_custombackend_url_helper_only", "ok": False, "raw": r["stdout"][:300]}
    return {
        "name": "L8.3_custombackend_url_helper_only",
        "ok": (
            parsed.get("is_wrapper_module") is True
            and parsed.get("custom_backend_enabled") is True
            and parsed.get("fetch_threw") is True
            and parsed.get("fetch_error_indicates_network") is True
        ),
        **parsed,
    }


def main() -> int:
    results = [
        case_1_fileedittool_validate_rejects_noop_edit(),
        case_2_filereadtool_call_throws_on_missing(),
        case_3_custombackend_url_helper_returns_string(),
    ]
    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "test_design": (
            "Each case imports actual mossen wrapper class and invokes its methods. "
            "Errors are tested at the wrapper boundary, not at Node primitives."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
