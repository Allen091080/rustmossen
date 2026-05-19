#!/usr/bin/env python3
"""
M14 — Browser/Chrome/computer-use 命令默认隐藏 (G5-2 必需).

按 GrowthBook迁移计划.md §G0-7 (G5-2 加 M14_browser_hidden_smoke).

契约:
  - mossen 个人版默认不展示 chrome/browser/computer-use 命令
  - registry 不含 'chrome', 'browser', 'computer-use' 等命令名 (或至少 isHidden=true)
  - 启动后无 hosted browser endpoint 请求 (复用 R7 mock 框架)

策略:
  - bun -e 调 getCommands() 真 enumerate, 黑名单不能命中
  - 启动 mossen -p 短对话, mock 任意 hosted host, 收 0 browser-related 请求

反测信号:
  - 改 commands.ts 把 chrome 命令注册回去 → 验失败
  - 删 G5-2 alias 让 tengu_chrome_auto_enable 真 fallback 到 GB → mock 收到请求
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log
from lib.mock_http_capture import MockCaptureServer, alloc_port


RUN_BUN = str(ROOT / "run-bun-featured.sh")

BROWSER_BLACKLIST = ["chrome", "browser", "computer-use",
                      "computer_use", "computeruse"]
BROWSER_PATH_TOKENS = ("/chrome", "/browser", "/computer", "/copper_bridge",
                        "/mossen_in_chrome")


def case_browser_commands_hidden() -> dict:
    """A: getCommands() 不含 browser/chrome/computer-use."""
    ctx = make_fixture("M14_A_browser_cmds_hidden")
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const { getCommands } = await import('./commands.ts');"
        "const cmds = await getCommands();"
        f"const blacklist = {json.dumps(BROWSER_BLACKLIST)};"
        "const present = blacklist.filter(n => cmds.find(c => "
        "  c.name === n || c.name.includes(n.replace('_', '-'))));"
        "process.stdout.write(JSON.stringify({"
        "  total: cmds.length,"
        "  browser_present: present,"
        "}));"
    )
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        env={**ctx.env, "MOSSEN_CONFIG_DIR": str(ctx.mossen_config_home)},
        capture_output=True, text=True, timeout=60, cwd=str(ROOT),
    )
    write_command_log(ctx, ["bun -e (M14 cmds enumerate)"],
                      proc.stdout, proc.stderr, proc.returncode)
    if proc.returncode != 0:
        return {"name": "M14_A_browser_cmds_hidden", "ok": False,
                "exit_code": proc.returncode, "stderr": proc.stderr[:500],
                "_ctx": ctx}
    try:
        data = json.loads(proc.stdout.strip())
    except json.JSONDecodeError as e:
        return {"name": "M14_A_browser_cmds_hidden", "ok": False,
                "json_error": str(e), "stdout": proc.stdout[:500], "_ctx": ctx}

    ok = isinstance(data.get("browser_present"), list) and len(data["browser_present"]) == 0
    return {
        "name": "M14_A_browser_cmds_hidden",
        "ok": ok,
        "total_cmds": data.get("total"),
        "browser_present": data.get("browser_present"),
        "_ctx": ctx,
    }


def case_no_browser_endpoint_traffic() -> dict:
    """B: 启动 mossen -p, mock 任意 hosted host, 验 browser endpoint 0 请求."""
    ctx = make_fixture("M14_B_no_browser_traffic")
    server = MockCaptureServer.start(port=alloc_port())
    try:
        env = dict(ctx.env)
        env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
        env["MOSSEN_NON_INTERACTIVE_SESSION"] = "1"
        env["MOSSEN_CODE_TRUST_DIALOG_ACCEPTED"] = "1"
        # 把 chrome bridge 也指向 mock (会失败但应该 0 请求才对)
        mock_url = f"http://127.0.0.1:{server.port}"
        env["MOSSEN_CODE_GB_BASE_URL"] = mock_url

        proj = ctx.root_dir / "fake_project"
        proj.mkdir(parents=True, exist_ok=True)
        proc = subprocess.run(
            [str(ROOT / "run-mossen.sh"), "-p"],
            input="请回复: M14_BROWSER_TEST_OK",
            env=env, capture_output=True, text=True, timeout=180, cwd=str(proj),
        )
        write_command_log(ctx, ["mossen -p (M14)"],
                          proc.stdout, proc.stderr, proc.returncode)
        time.sleep(3)
        all_reqs = server.received
    finally:
        server.stop()

    browser_reqs = [r for r in all_reqs
                    if any(t in r["path"] for t in BROWSER_PATH_TOKENS)]
    base_ok = proc.returncode == 0 and "M14_BROWSER_TEST_OK" in proc.stdout
    ok = base_ok and len(browser_reqs) == 0
    return {
        "name": "M14_B_no_browser_traffic",
        "ok": ok,
        "exit_code": proc.returncode,
        "browser_request_count": len(browser_reqs),
        "browser_request_excerpt": browser_reqs[:3],
        "stdout_excerpt": proc.stdout[:200],
        "_ctx": ctx,
    }


def main() -> int:
    cases = [case_browser_commands_hidden(), case_no_browser_endpoint_traffic()]
    for c in cases:
        ctx = c.pop("_ctx")
        write_assertions(ctx,
                         status="passed" if c.get("ok") else "failed",
                         assertions=[{
                             "name": c["name"],
                             "expected": True,
                             "actual": c.get("ok"),
                             "passed": c.get("ok"),
                             "evidence": json.dumps(
                                 {k: v for k, v in c.items() if k != "name"}
                             )[:500],
                         }])
    overall_ok = all(c.get("ok") for c in cases)
    print(json.dumps({
        "results": cases,
        "passed": sum(1 for c in cases if c.get("ok")),
        "total": len(cases),
        "design_note": (
            "M14: browser/chrome/computer-use 命令默认隐藏 + "
            "启动后 0 browser endpoint 请求 (G0-7 G5-2 必需)."
        ),
    }, indent=2, ensure_ascii=False, default=str))
    return 0 if overall_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
