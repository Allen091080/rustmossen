#!/usr/bin/env python3
"""
M3.4 — MCP scope 可见性 + 坏 server 隔离 e2e。

按 harness全链路测试.md §3.3 M3.4 契约:
  策略 (2 case):
    case_user_and_project_visible:
      前置: user-scope (~/.mossen.json mcpServers) + project-scope (<cwd>/.mcp.json) 各注册 1 个 mock server
      步骤: bun -e 调真 getMcpConfigsByScope('user') / ('project') 列举
      观察点: 'user' scope 含 m34_user_server, 'project' scope 含 m34_project_server
      反测: src/services/mcp/config.ts:getMcpConfigsByScope 'user' 分支 return 空 → user 不可见 → fail

    case_bad_server_isolated:
      前置: project .mcp.json 含 1 好 (mock_mcp_server) + 1 坏 (command 指向 /bin/this-binary-does-not-exist-M34)
      步骤: 跑 'mossen mcp list' (cwd=fixture root) 验启动不 crash, 好/坏 server 都被列出
      观察点: exit_code 0, stdout 含好 server 名, stdout 含坏 server 名 (list 不因坏 server 漏)
      反测: src/services/mcp/config.ts 改让 spawn 失败时 throw → mossen mcp list 非 0 exit → fail

实现策略:
  case 1 走 bun -e (内部真 API), 避免 mossen mcp list 不区分 scope 字面.
  case 2 走 mossen mcp list (真 CLI 路径 + 启动不崩).

  真实导出名 (已 grep 验证):
    getMcpConfigsByScope ← src/services/mcp/config.ts:889
    enableConfigs        ← src/utils/config.ts (M7.1/M7.2 已用)
  global file 路径: <MOSSEN_CONFIG_DIR>/.mossen.json (默认无 oauth suffix → prod)
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
RUN_MOSSEN = str(ROOT / "run-mossen.sh")

USER_SERVER_NAME = "m34_user_server"
PROJECT_SERVER_NAME = "m34_project_server"
GOOD_SERVER_NAME = "m34_good_server"
BAD_SERVER_NAME = "m34_bad_server"
BAD_BINARY_PATH = "/bin/this-binary-does-not-exist-M34-zzz"


def case_user_and_project_visible() -> dict:
    ctx = make_fixture("M3.4-scope")

    mock_server_path = ROOT / "scripts" / "harness_mock_mcp_server.py"

    # 1. 写 user-scope 配置: <MOSSEN_CONFIG_DIR>/.mossen.json
    # 该文件必须包含 numStartups (基础字段) — 但 saveConfig 也接受空 object,
    # 我们只写 mcpServers 让 getGlobalConfig().mcpServers 能取到.
    user_global_file = ctx.mossen_config_home / ".mossen.json"
    user_global_file.write_text(
        json.dumps(
            {
                "mcpServers": {
                    USER_SERVER_NAME: {
                        "type": "stdio",
                        "command": "python3",
                        "args": [str(mock_server_path)],
                    },
                },
            },
            indent=2,
        ),
        encoding="utf-8",
    )

    # 2. 写 project-scope 配置: <cwd>/.mcp.json
    project_mcp_file = ctx.root_dir / ".mcp.json"
    project_mcp_file.write_text(
        json.dumps(
            {
                "mcpServers": {
                    PROJECT_SERVER_NAME: {
                        "type": "stdio",
                        "command": "python3",
                        "args": [str(mock_server_path)],
                    },
                },
            },
            indent=2,
        ),
        encoding="utf-8",
    )

    # 3. bun -e: chdir 到 fixture root, 调 getMcpConfigsByScope 真 API
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        f"process.chdir({json.dumps(str(ctx.root_dir))});"
        "const { setOriginalCwd, setProjectRoot, setCwdState } = await import('./bootstrap/state.ts');"
        f"setOriginalCwd({json.dumps(str(ctx.root_dir))});"
        f"setProjectRoot({json.dumps(str(ctx.root_dir))});"
        f"setCwdState({json.dumps(str(ctx.root_dir))});"
        "import { getMcpConfigsByScope } from './services/mcp/config.ts';"
        "const userScope = getMcpConfigsByScope('user');"
        "const projectScope = getMcpConfigsByScope('project');"
        "process.stdout.write(JSON.stringify({"
        "  userServers: Object.keys(userScope.servers || {}),"
        "  userErrors: (userScope.errors || []).map(e => e && e.message),"
        "  projectServers: Object.keys(projectScope.servers || {}),"
        "  projectErrors: (projectScope.errors || []).map(e => e && e.message),"
        "}) + '\\n');"
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
    )

    write_command_log(
        ctx,
        [RUN_BUN, "-e", "<getMcpConfigsByScope user/project>"],
        proc.stdout,
        proc.stderr,
        proc.returncode,
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

    if not parsed:
        return {
            "name": "user_and_project_visible",
            "ok": False,
            "exit_code": proc.returncode,
            "stdout_excerpt": (proc.stdout or "")[:500],
            "stderr_excerpt": (proc.stderr or "")[:500],
            "fixture_root": str(ctx.root_dir),
            "_ctx": ctx,
        }

    user_servers = parsed.get("userServers") or []
    project_servers = parsed.get("projectServers") or []
    user_visible = USER_SERVER_NAME in user_servers
    project_visible = PROJECT_SERVER_NAME in project_servers

    return {
        "name": "user_and_project_visible",
        "ok": (
            proc.returncode == 0
            and user_visible
            and project_visible
        ),
        "exit_code": proc.returncode,
        "user_visible": user_visible,
        "project_visible": project_visible,
        "user_servers": user_servers,
        "project_servers": project_servers,
        "user_errors": parsed.get("userErrors"),
        "project_errors": parsed.get("projectErrors"),
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def case_bad_server_isolated() -> dict:
    ctx = make_fixture("M3.4-isolated")

    mock_server_path = ROOT / "scripts" / "harness_mock_mcp_server.py"

    # project .mcp.json: 1 好 + 1 坏
    project_mcp_file = ctx.root_dir / ".mcp.json"
    project_mcp_file.write_text(
        json.dumps(
            {
                "mcpServers": {
                    GOOD_SERVER_NAME: {
                        "type": "stdio",
                        "command": "python3",
                        "args": [str(mock_server_path)],
                    },
                    BAD_SERVER_NAME: {
                        "type": "stdio",
                        "command": BAD_BINARY_PATH,
                        "args": ["whatever"],
                    },
                },
            },
            indent=2,
        ),
        encoding="utf-8",
    )

    proc = subprocess.run(
        [RUN_MOSSEN, "mcp", "list"],
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=90,
        cwd=str(ctx.root_dir),
    )

    write_command_log(
        ctx,
        ["mossen", "mcp", "list"],
        proc.stdout,
        proc.stderr,
        proc.returncode,
    )

    good_in_stdout = GOOD_SERVER_NAME in proc.stdout
    bad_in_stdout = BAD_SERVER_NAME in proc.stdout
    # mossen 自己未崩 (启动到 list 输出完整)
    started_ok = proc.returncode == 0

    return {
        "name": "bad_server_isolated",
        "ok": (
            started_ok
            and good_in_stdout
            and bad_in_stdout
        ),
        "exit_code": proc.returncode,
        "started_ok": started_ok,
        "good_in_stdout": good_in_stdout,
        "bad_in_stdout": bad_in_stdout,
        "stdout_excerpt": (proc.stdout or "")[:600],
        "stderr_excerpt": (proc.stderr or "")[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = case_user_and_project_visible()
    res2 = case_bad_server_isolated()
    ctx_main = res1.pop("_ctx")
    res2.pop("_ctx")
    results = [res1, res2]

    write_assertions(
        ctx_main,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {
                "name": r["name"],
                "expected": True,
                "actual": r.get("ok"),
                "passed": r.get("ok"),
                "evidence": (
                    f"user_visible={r.get('user_visible')} "
                    f"project_visible={r.get('project_visible')}"
                    if r["name"] == "user_and_project_visible"
                    else
                    f"started_ok={r.get('started_ok')} "
                    f"good={r.get('good_in_stdout')} "
                    f"bad={r.get('bad_in_stdout')}"
                ),
            }
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx_main.root_dir),
        "design_note": (
            "M3.4 scope+isolation: getMcpConfigsByScope('user'/'project') "
            "must surface both servers; mossen mcp list must list both good "
            "and bad project servers without crashing (bad spawn isolated)."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
