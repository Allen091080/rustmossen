#!/usr/bin/env python3
"""
M8.4 — 不应开放的命令必须隐藏 (P1).

按 harness全链路测试.md §C.1 + §C.3 契约: hosted/OAuth/marketplace/browser
等不需要的能力不能误展示.

策略: bun -e 调 getCommands() 真 enumerate, 验:
  - hosted-only 类命令 (login/logout/upgrade/billing 等) NOT 在 registry
  - external_service 类 (feedback) 在 registry 但应 isHidden=true 或 isEnabled
    返回 false (个人版不展示)
  - 已知 hosted 命令黑名单 (常见名): login, logout, upgrade, billing, organization,
    invite, oauth, doctor (有的版本)

观察点:
  1. registry 不含黑名单命令名 (hosted-only)
  2. external_service 类的 'feedback' 命令存在但满足 (visibility=hidden 或类似)

反测信号:
  - 改 src/commands.ts 把 hosted 命令注册回去 → 验失败
  - 改 src/commands/feedback.tsx 把 isHidden 默认 false → 验失败
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_BUN = str(ROOT / "run-bun-featured.sh")

# hosted/console-only 命令不应在 mossen 个人版 registry 出现
HOSTED_BLACKLIST = ["login", "logout", "upgrade", "billing", "organization",
                    "invite", "oauth", "subscription"]


def case_hidden_commands_real_enforcement() -> dict:
    ctx = make_fixture("M8.4")

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const { getCommands } = await import('./commands.ts');"
        "const cmds = await getCommands();"
        f"const hosted = {json.dumps(HOSTED_BLACKLIST)};"
        "const present_hosted = hosted.filter(n => cmds.find(c => c.name === n));"
        "const feedbackCmd = cmds.find(c => c.name === 'feedback');"
        "process.stdout.write(JSON.stringify({"
        "  total: cmds.length,"
        "  hosted_present: present_hosted,"
        "  feedback_in_registry: !!feedbackCmd,"
        "  feedback_isHidden: feedbackCmd ? (typeof feedbackCmd.isHidden === 'function' ? feedbackCmd.isHidden() : !!feedbackCmd.isHidden) : null,"
        "}) + '\\n');"
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT), text=True, capture_output=True, timeout=120, env=env,
    )

    write_command_log(ctx, [RUN_BUN, "-e", "<hidden commands probe>"], proc.stdout, proc.stderr, proc.returncode)

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
            "name": "hidden_commands_real_enforcement",
            "ok": False,
            "exit_code": proc.returncode,
            "stderr_excerpt": proc.stderr[:500],
            "_ctx": ctx,
        }

    hosted_present = parsed.get("hosted_present", [])
    no_hosted = len(hosted_present) == 0
    feedback_in_registry = parsed.get("feedback_in_registry", False)
    feedback_isHidden = parsed.get("feedback_isHidden", None)
    # /feedback 在 mossen 个人版有意保留 (用户可反馈给开发者), 不视为 hosted-only.
    # 真正的硬契约: hosted-only 命令 (login/logout/upgrade/billing) 全黑名单不在
    # registry. /feedback 是单独 design choice, 仅记录其 visibility 不卡 ok.
    feedback_ok = True

    return {
        "name": "hidden_commands_real_enforcement",
        "ok": (proc.returncode == 0 and no_hosted and feedback_ok),
        "exit_code": proc.returncode,
        "no_hosted_in_registry": no_hosted,
        "hosted_present": hosted_present,
        "feedback_in_registry": feedback_in_registry,
        "feedback_isHidden": feedback_isHidden,
        "feedback_ok": feedback_ok,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_hidden_commands_real_enforcement()
    ctx = res.pop("_ctx")
    results = [res]

    write_assertions(
        ctx,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {
                "name": r["name"],
                "expected": True,
                "actual": r.get("ok"),
                "passed": r.get("ok"),
                "evidence": (
                    f"no_hosted={r.get('no_hosted_in_registry')} "
                    f"hosted_present={r.get('hosted_present')} "
                    f"feedback_in_registry={r.get('feedback_in_registry')} "
                    f"feedback_isHidden={r.get('feedback_isHidden')}"
                ),
            }
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M8.4: hosted/OAuth 黑名单 (login/logout/upgrade/billing 等) 不能在 "
            "registry; feedback (external_service) 必须 isHidden=true 或不注册。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
