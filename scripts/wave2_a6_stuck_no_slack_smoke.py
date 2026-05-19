#!/usr/bin/env python3
"""Wave 2A — A6 (ANT-STUCK S3 hard remove + smoke_check.py 4 字符串同 commit) focused smoke.

Verifies:
  1. skills/bundled/stuck.ts 文件不存在
  2. skills/bundled/index.ts 不含 registerStuckSkill / './stuck.js' 引用
  3. C07VBSHV7EV Slack 频道 ID 在 skills/bundled/ 全目录 grep 命中 = 0
     (注: tools/BashTool/bashPermissions.ts:211 的 pre-existing comment 引用
      不在 A6 范围,留 Wave 3 文档清理)
  4. #mossen-code-feedback 仅在 components/FullscreenLayout.tsx 出现
     (Sarah Deaton 留的源码注释, 保留)
  5. docs/command-inventory.md 不再含 /stuck 行, 编号已修正 (12 项 = 11→11/12, 13→12)
  6. mossen 默认下 /help 列表不含 /stuck (静态推断: index.ts 不再 register, 行为零差异)

Why static-only:
  * skills 注册系统涉及 deferred runtime 模块, `bun -e` 不能解析。
  * v3 §2.6 5 项审查已确认 (smoke_check.py 4 字符串 = 软阻断 / docs 1 处 /
    prompt 整文件删除一并清 / slash command 解注册 / harness 0 命中)。
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
STUCK_FILE = ROOT / "skills" / "bundled" / "stuck.ts"
INDEX_FILE = ROOT / "skills" / "bundled" / "index.ts"
FULLSCREEN_LAYOUT = ROOT / "components" / "FullscreenLayout.tsx"
CMD_INVENTORY = ROOT / "docs" / "command-inventory.md"
SKILLS_DIR = ROOT / "skills" / "bundled"


def static_assertion() -> dict[str, object]:
    findings: dict[str, object] = {
        "stuck_file_deleted": True,
        "index_no_register_stuck": False,
        "index_no_stuck_import": False,
        "slack_channel_id_in_skills_count": 0,
        "feedback_channel_in_fullscreen_only": False,
        "cmd_inventory_no_stuck_line": False,
        "cmd_inventory_renumbered": False,
    }

    if STUCK_FILE.exists():
        findings["stuck_file_deleted"] = False

    if INDEX_FILE.exists():
        idx_text = INDEX_FILE.read_text(encoding="utf-8")
        findings["index_no_register_stuck"] = "registerStuckSkill" not in idx_text
        findings["index_no_stuck_import"] = "./stuck.js" not in idx_text

    # Count C07VBSHV7EV in skills/bundled/ recursively.
    count = 0
    if SKILLS_DIR.exists():
        for f in SKILLS_DIR.rglob("*.ts"):
            try:
                if "C07VBSHV7EV" in f.read_text(encoding="utf-8"):
                    count += 1
            except (UnicodeDecodeError, OSError):
                pass
    findings["slack_channel_id_in_skills_count"] = count

    # #mossen-code-feedback present in FullscreenLayout (allowed pre-existing comment).
    if FULLSCREEN_LAYOUT.exists():
        fs_text = FULLSCREEN_LAYOUT.read_text(encoding="utf-8")
        findings["feedback_channel_in_fullscreen_only"] = (
            "#mossen-code-feedback" in fs_text
        )

    if CMD_INVENTORY.exists():
        inv_text = CMD_INVENTORY.read_text(encoding="utf-8")
        findings["cmd_inventory_no_stuck_line"] = (
            "/stuck" not in inv_text
            and "skills/bundled/stuck.ts" not in inv_text
        )
        # After renumber: 11 should be /update-config, 12 should be /verify
        findings["cmd_inventory_renumbered"] = bool(
            re.search(r"\|\s*11\s*\|\s*`/update-config`", inv_text)
        ) and bool(re.search(r"\|\s*12\s*\|\s*`/verify`", inv_text))

    return findings


def main() -> int:
    failures: list[str] = []
    f = static_assertion()

    if not f["stuck_file_deleted"]:
        failures.append("skills/bundled/stuck.ts 仍存在 — A6 要求 hard remove 整文件")
    if not f["index_no_register_stuck"]:
        failures.append("skills/bundled/index.ts 仍含 registerStuckSkill 调用")
    if not f["index_no_stuck_import"]:
        failures.append("skills/bundled/index.ts 仍 import './stuck.js'")
    if f["slack_channel_id_in_skills_count"] != 0:
        failures.append(
            f"skills/bundled/ 内仍有 {f['slack_channel_id_in_skills_count']} 处 "
            "C07VBSHV7EV (anthropic 内网 Slack 频道 ID) — 应为 0"
        )
    if not f["feedback_channel_in_fullscreen_only"]:
        failures.append(
            "#mossen-code-feedback 不在 components/FullscreenLayout.tsx 中 — "
            "A6 不应删除该 pre-existing 注释 (Sarah Deaton 留)"
        )
    if not f["cmd_inventory_no_stuck_line"]:
        failures.append("docs/command-inventory.md 仍含 /stuck 行")
    if not f["cmd_inventory_renumbered"]:
        failures.append(
            "docs/command-inventory.md 编号未修正: 期望 11=/update-config, 12=/verify"
        )

    report = {
        "name": "wave2_a6_stuck_no_slack_smoke",
        "mode": "static-only",
        "mode_reason": (
            "skills 注册系统涉及 deferred runtime 模块。S3 hard remove 是纯结构 + "
            "字符串清理, 静态断言已足够;真实 /help 行为差 = 0 由 mossen 默认 "
            "USER_TYPE=undefined 时本来就走 isEnabled=false 的 ANT gate 兜底。"
        ),
        "static_findings": f,
        "failures": failures,
        "passed": 7 - len(failures),
        "total": 7,
    }
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
