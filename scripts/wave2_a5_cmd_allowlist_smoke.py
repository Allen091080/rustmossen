#!/usr/bin/env python3
"""Wave 2A — A5 (MOS-CMD-ALLOWLIST) focused smoke (static-only).

Verifies C-2 拆分:
  * 22 个 gh 子命令 (GH_READ_ONLY_COMMANDS spread) 已合并到通用 COMMAND_ALLOWLIST,
    所有 USER_TYPE 都可用,不再 USER_TYPE=mossen 门控
  * 1 个 aki (anthropic 内部 KB 检索 CLI) S3 hard remove
  * MOSSEN_ONLY_COMMAND_ALLOWLIST 整体定义已删除
  * getCommandAllowlist() 内 USER_TYPE=mossen 分支已删除
  * tools/PowerShellTool/readOnlyValidation.ts isGhSafe() USER_TYPE=mossen 守门已删除

Why static-only:
  * BashTool/PowerShellTool readOnlyValidation 透传依赖与 bashPermissions.ts 同
    deferred 子模块链 — `bun -e` 解析失败。
  * v3 §2.3 case 设计为静态结构断言:每个 case 直接检查 *.ts 内特定字符串/语法的
    存在/缺失;真实运行行为由 TUI 集成 smoke 兜底。

case 草案 (v3 §2.3.7 — A 至 F 共 6):
  A: BashTool COMMAND_ALLOWLIST 含 ...GH_READ_ONLY_COMMANDS (公共 allowlist)
  B: aki 字面量在 readOnlyValidation.ts 中已不存在 (S3 deleted)
  C: ghIsDangerousCallback 仍存在 (回归 — 三段式 HOST/OWNER/REPO + URL + SSH 拦截)
  D: --show-token flag 拒绝模式仍存在 (回归)
  E: USER_TYPE=mossen 行为一致性 — getCommandAllowlist 已无 USER_TYPE 分支
  F: PowerShell isGhSafe 内已无 USER_TYPE !== 'mossen' 早返回

Note: utils/attachments.ts:546 'aki' 是 skill_discovery attachment source 字段
(同名巧合, **不要误删**) — smoke 不检查该路径。
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASH_RO = ROOT / "tools" / "BashTool" / "readOnlyValidation.ts"
PS_RO = ROOT / "tools" / "PowerShellTool" / "readOnlyValidation.ts"
GH_DEF = ROOT / "utils" / "shell" / "readOnlyCommandValidation.ts"


def static_assertion() -> dict[str, object]:
    bash = BASH_RO.read_text(encoding="utf-8")
    ps = PS_RO.read_text(encoding="utf-8")

    findings: dict[str, object] = {
        "case_A_gh_in_public_allowlist": False,
        "case_B_aki_removed": True,  # default true → fail unless leak found
        "case_C_ghIsDangerousCallback_present": False,
        "case_D_show_token_rejected": False,
        "case_E_command_allowlist_no_usertype_gate": False,
        "case_F_powershell_isghsafe_no_usertype_gate": False,
        "mossen_only_command_allowlist_def_present": True,  # default true
    }

    # Case A — COMMAND_ALLOWLIST contains GH_READ_ONLY_COMMANDS spread.
    cmd_block_re = re.compile(
        r"const COMMAND_ALLOWLIST: Record<string, CommandConfig> = \{(?P<body>[\s\S]*?)\}\s*\n\s*\n",
        re.MULTILINE,
    )
    m = cmd_block_re.search(bash)
    if m:
        body = m.group("body")
        if "...GH_READ_ONLY_COMMANDS" in body:
            findings["case_A_gh_in_public_allowlist"] = True

    # Case B — aki definition gone from BashTool readOnlyValidation.
    # Check there's no `  aki: {` block (definition leak)
    if re.search(r"^\s*aki\s*:\s*\{", bash, re.MULTILINE):
        findings["case_B_aki_removed"] = False
    # Also confirm '--keyword' from aki block isn't still present
    if "'--keyword'" in bash:
        findings["case_B_aki_removed"] = False

    # Case C — ghIsDangerousCallback still defined / referenced (regression check).
    # Look for a function or callback with that identifier across the codebase.
    bash_has_gh_dangerous = bool(re.search(r"ghIsDangerousCallback\b", bash))
    # Sometimes lives in the gh definition module instead.
    gh_def = GH_DEF.read_text(encoding="utf-8") if GH_DEF.exists() else ""
    if bash_has_gh_dangerous or "ghIsDangerous" in gh_def:
        findings["case_C_ghIsDangerousCallback_present"] = True

    # Case D — --show-token rejection still in place (regression).
    # GH_READ_ONLY_COMMANDS in utils/shell/readOnlyCommandValidation.ts has
    # `--show-token` listed under unsafeFlags / rejected list.
    if "'--show-token'" in gh_def or "show-token" in gh_def:
        findings["case_D_show_token_rejected"] = True

    # Case E — getCommandAllowlist no longer branches on USER_TYPE=mossen.
    fn_re = re.compile(
        r"function getCommandAllowlist\(\)[\s\S]*?\n\}",
        re.MULTILINE,
    )
    fm = fn_re.search(bash)
    if fm:
        fn_body = fm.group(0)
        findings["case_E_command_allowlist_no_usertype_gate"] = (
            "USER_TYPE" not in fn_body and "MOSSEN_ONLY_COMMAND_ALLOWLIST" not in fn_body
        )

    # Also confirm MOSSEN_ONLY_COMMAND_ALLOWLIST identifier is fully gone from file.
    findings["mossen_only_command_allowlist_def_present"] = (
        "MOSSEN_ONLY_COMMAND_ALLOWLIST" in bash
    )

    # Case F — PowerShell isGhSafe no USER_TYPE gate.
    is_gh_re = re.compile(
        r"function isGhSafe\(args: string\[\]\): boolean \{(?P<body>[\s\S]*?)\n\}",
        re.MULTILINE,
    )
    pm = is_gh_re.search(ps)
    if pm:
        body = pm.group("body")
        # Look at first ~600 chars for early-return USER_TYPE check.
        # Simpler: assert no USER_TYPE !== 'mossen' return false pattern.
        no_gate = (
            "USER_TYPE !== 'mossen'" not in body
            and "USER_TYPE === 'mossen'" not in body
        )
        findings["case_F_powershell_isghsafe_no_usertype_gate"] = no_gate

    return findings


def main() -> int:
    failures: list[str] = []
    f = static_assertion()

    if not f["case_A_gh_in_public_allowlist"]:
        failures.append(
            "Case A FAIL: BashTool COMMAND_ALLOWLIST 内未发现 ...GH_READ_ONLY_COMMANDS — "
            "C-2 拆分要求把 22 个 gh 子命令提到公共 allowlist"
        )
    if not f["case_B_aki_removed"]:
        failures.append(
            "Case B FAIL: aki 命令定义/相关 flag 仍残留 — S3 hard remove 不彻底"
        )
    if not f["case_C_ghIsDangerousCallback_present"]:
        failures.append(
            "Case C FAIL: ghIsDangerousCallback / ghIsDangerous 防御回归丢失 — "
            "C-2 拆分不允许削弱 exfil 防御"
        )
    if not f["case_D_show_token_rejected"]:
        failures.append(
            "Case D FAIL: --show-token 拒绝列表丢失 — 回归"
        )
    if not f["case_E_command_allowlist_no_usertype_gate"]:
        failures.append(
            "Case E FAIL: getCommandAllowlist 内仍含 USER_TYPE 分支或 "
            "MOSSEN_ONLY_COMMAND_ALLOWLIST 引用 — gate 未解"
        )
    if f["mossen_only_command_allowlist_def_present"]:
        failures.append(
            "MOSSEN_ONLY_COMMAND_ALLOWLIST 标识符仍出现在 BashTool/readOnlyValidation.ts — "
            "C-2 要求删除整个 const 定义"
        )
    if not f["case_F_powershell_isghsafe_no_usertype_gate"]:
        failures.append(
            "Case F FAIL: PowerShell isGhSafe 仍含 USER_TYPE 守门 — 同步未做"
        )

    report = {
        "name": "wave2_a5_cmd_allowlist_smoke",
        "mode": "static-only",
        "mode_reason": (
            "BashTool/PowerShellTool readOnlyValidation 透传 deferred runtime 子模块 — "
            "`bun -e` 解析失败。C-2 拆分 + S3 删除属纯结构改动, 静态断言已足够。"
        ),
        "static_findings": f,
        "failures": failures,
        "passed": 6 - len(failures),
        "total": 6,
    }
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
