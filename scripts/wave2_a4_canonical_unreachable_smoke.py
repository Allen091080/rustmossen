#!/usr/bin/env python3
"""Wave 2A — A4 (MOS-CANONICAL S3 hard remove) focused smoke (static-only).

Verifies that 5 H-anchors + 3 副作用面 in tools/SkillTool/SkillTool.ts have
been hard removed:

H-anchors:
  H0 — 顶层 remoteSkillModules conditional require (`services/skillSearch/*`)
  H1 — validateInput 中 `_canonical_<slug>` 解析
  H2 — checkPermissions 中 canonical skill 自动 allow
  H3 — call() 中 `_canonical_<slug>` 拦截
  H4 — executeRemoteSkill 函数 (140+ 行) + extractUrlScheme

副作用面 (A1/A2/A3):
  A1 — executeForkedSkill 中 was_discovered 字段
  A2 — call() inline path 中同一 was_discovered 字段
  A3 — extractUrlScheme (在 H4 范围内)

并附 3 防回归断言 (v3 §2.5.7):
  防回归 1: `_canonical_xyz` 调用走 `errorCode 2 Unknown skill` fallback
  防回归 2: SkillTool prompt 不提 canonical
  防回归 3: `/skills` `/skillify` 命令不依赖 remoteSkillModules

Why static-only:
  * SkillTool.ts 透传依赖含 deferred runtime 模块, `bun -e` 解析不到 source
  * S3 hard remove 是纯结构 + 引用清理, 静态断言已足够
  * v3 §2.5 显式确认 5 项审查 (测试 0 / 文档 0 *.md / prompt 不提 canonical /
    slash command 0 影响 / harness 0 命中)

SA-4 物理崩溃脚枪佐证: services/skillSearch/* 4 子模块在 mossen 仓内不存在,
即使误开 EXPERIMENTAL_SKILL_SEARCH gate, build 也立即崩。删除 H0-H4 后该
gate 在 SkillTool.ts 全文 0 引用。
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SKILL_TOOL = ROOT / "tools" / "SkillTool" / "SkillTool.ts"
SKILL_PROMPT = ROOT / "tools" / "SkillTool" / "prompt.ts"
COMMANDS_TS = ROOT / "commands.ts"


def static_assertion() -> dict[str, object]:
    text = SKILL_TOOL.read_text(encoding="utf-8")
    prompt_text = SKILL_PROMPT.read_text(encoding="utf-8") if SKILL_PROMPT.exists() else ""
    cmds_text = COMMANDS_TS.read_text(encoding="utf-8") if COMMANDS_TS.exists() else ""

    findings: dict[str, object] = {
        "H0_remoteSkillModules_const_removed": True,
        "H1_validateInput_canonical_intercept_removed": True,
        "H2_checkPermissions_canonical_autoAllow_removed": True,
        "H3_call_canonical_intercept_removed": True,
        "H4_executeRemoteSkill_function_removed": True,
        "H4_extractUrlScheme_function_removed": True,
        "A1_was_discovered_in_executeForkedSkill_removed": True,
        "A2_was_discovered_in_call_inline_removed": True,
        "experimental_skill_search_zero_runtime_uses": True,
        # Regression checks
        "regression_unknown_skill_fallback_present": False,
        "regression_prompt_no_canonical": False,
        "regression_skill_commands_no_remoteSkillModules": False,
    }

    # H0: const remoteSkillModules = feature('EXPERIMENTAL_SKILL_SEARCH') ?
    if re.search(r"^const remoteSkillModules\s*=", text, re.MULTILINE):
        findings["H0_remoteSkillModules_const_removed"] = False
    if "remoteSkillModules!" in text or "remoteSkillModules?" in text:
        findings["H0_remoteSkillModules_const_removed"] = False

    # H1: validateInput canonical intercept (look for stripCanonicalPrefix in non-comment)
    # Strip comment lines and check.
    code_only = "\n".join(
        line for line in text.splitlines() if not line.lstrip().startswith("//")
    )
    if "stripCanonicalPrefix" in code_only:
        findings["H1_validateInput_canonical_intercept_removed"] = False
        findings["H2_checkPermissions_canonical_autoAllow_removed"] = False
        findings["H3_call_canonical_intercept_removed"] = False

    # H4: executeRemoteSkill function definition
    if re.search(r"\bfunction\s+executeRemoteSkill\b", code_only):
        findings["H4_executeRemoteSkill_function_removed"] = False
    if re.search(r"\bexecuteRemoteSkill\s*\(", code_only):
        findings["H4_executeRemoteSkill_function_removed"] = False

    # H4: extractUrlScheme function
    if re.search(r"\bfunction\s+extractUrlScheme\b", code_only):
        findings["H4_extractUrlScheme_function_removed"] = False
    if re.search(r"\bextractUrlScheme\s*\(", code_only):
        findings["H4_extractUrlScheme_function_removed"] = False

    # A1/A2: was_discovered field in code (not comments)
    if re.search(r"\bwas_discovered\s*:", code_only):
        findings["A1_was_discovered_in_executeForkedSkill_removed"] = False
        findings["A2_was_discovered_in_call_inline_removed"] = False
    if re.search(r"\bwasDiscoveredField\b", code_only):
        findings["A1_was_discovered_in_executeForkedSkill_removed"] = False
        findings["A2_was_discovered_in_call_inline_removed"] = False

    # EXPERIMENTAL_SKILL_SEARCH 在 SkillTool.ts 中 0 runtime 引用 (注释允许)
    if re.search(r"\bfeature\(['\"]EXPERIMENTAL_SKILL_SEARCH['\"]\)", code_only):
        findings["experimental_skill_search_zero_runtime_uses"] = False

    # 防回归 1: `errorCode 2 Unknown skill` fallback 路径仍存在
    if re.search(r"errorCode:\s*2", text) and "Unknown skill" in text:
        findings["regression_unknown_skill_fallback_present"] = True

    # 防回归 2: SkillTool prompt 不提 canonical
    findings["regression_prompt_no_canonical"] = (
        "canonical" not in prompt_text.lower() and "_canonical_" not in prompt_text
    )

    # 防回归 3: commands.ts 仍可能含 require('./services/skillSearch/...') 死路径
    # (v3 §2.5 留 Wave 3 处理), 不在 SkillTool.ts 内引用 remoteSkillModules
    findings["regression_skill_commands_no_remoteSkillModules"] = (
        "remoteSkillModules" not in cmds_text
    )

    return findings


def main() -> int:
    failures: list[str] = []
    f = static_assertion()

    must_be_true = [
        ("H0_remoteSkillModules_const_removed", "H0 顶层 remoteSkillModules conditional require 仍存在"),
        ("H1_validateInput_canonical_intercept_removed", "H1 validateInput canonical 拦截仍存在"),
        ("H2_checkPermissions_canonical_autoAllow_removed", "H2 checkPermissions canonical 自动 allow 仍存在"),
        ("H3_call_canonical_intercept_removed", "H3 call canonical 拦截仍存在"),
        ("H4_executeRemoteSkill_function_removed", "H4 executeRemoteSkill 函数仍存在"),
        ("H4_extractUrlScheme_function_removed", "H4 extractUrlScheme 函数仍存在"),
        ("A1_was_discovered_in_executeForkedSkill_removed", "A1/A2 was_discovered 字段仍存在"),
        ("A2_was_discovered_in_call_inline_removed", "A2 was_discovered (call inline) 仍存在"),
        ("experimental_skill_search_zero_runtime_uses", "EXPERIMENTAL_SKILL_SEARCH gate 在 SkillTool.ts 仍有 runtime 引用"),
        ("regression_unknown_skill_fallback_present", "防回归 1 失败: errorCode 2 Unknown skill fallback 路径丢失"),
        ("regression_prompt_no_canonical", "防回归 2 失败: SkillTool prompt 仍提 canonical"),
        ("regression_skill_commands_no_remoteSkillModules", "防回归 3 失败: commands.ts 内仍引用 remoteSkillModules"),
    ]
    for key, msg in must_be_true:
        if not f.get(key):
            failures.append(f"{key}: {msg}")

    report = {
        "name": "wave2_a4_canonical_unreachable_smoke",
        "mode": "static-only",
        "mode_reason": (
            "SkillTool.ts 透传依赖含 deferred runtime 模块。S3 hard remove 是纯结构 + "
            "引用清理, 静态断言已足够;真实运行行为由 TUI smoke 兜底 (case 39 等)。"
            "SA-4 物理崩溃脚枪: services/skillSearch/* 子模块本来就不存在, 删除主面后 "
            "EXPERIMENTAL_SKILL_SEARCH gate 在 SkillTool.ts 全文 0 引用。"
        ),
        "static_findings": f,
        "failures": failures,
        "passed": len(must_be_true) - len(failures),
        "total": len(must_be_true),
    }
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
