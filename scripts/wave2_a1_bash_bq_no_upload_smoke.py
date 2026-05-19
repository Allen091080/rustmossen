#!/usr/bin/env python3
"""Wave 2A — A1 (MOS-BASH-BQ S2) focused smoke (static-only).

Verifies that logClassifierResultForMossen in tools/BashTool/bashPermissions.ts
has been emptied to a single `return` statement (S2 阶段:保留签名,移除 BQ
上报副作用),while:
  * 函数签名仍存在 (后续 Wave 3 才 hard remove)
  * 4 处 callsites 仍调用此函数 (signature 保留正是为了不动 callsite)
  * tengu_internal_bash_classifier_result 字面量在该函数体内已消失
  * AnalyticsMetadata_I_VERIFIED... import 已删 (仅在被置空函数内被引用)
  * jsonStringify import 已删 (同上)
  * logEvent import 仍保留 (该文件另有 3 处合法调用 — line 1700/1725/2335)

Why static-only (not runtime):
  * tools/BashTool/bashPermissions.ts 透传依赖含 deferred runtime 模块 (与
    Wave 0 PERM-2 / API-001 同模式), `bun -e` 解析源码时无法装配,
    会以 "Cannot find module ./tools/REPLTool/REPLTool.js" 之类报错。
  * S2 阶段是纯结构改动 ("函数体置空"),静态结构断言已足够。
  * 真实运行行为 (bash 命令上报不再触发 logEvent) 由 TUI 集成 smoke
    + 全 smoke (38/39) 兜底,不需要在此本地 eval。

No LLM, no real backend, no ~/.mossen write. Pure file read + regex.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "tools" / "BashTool" / "bashPermissions.ts"


def static_assertion() -> dict[str, object]:
    text = TARGET.read_text(encoding="utf-8")

    findings: dict[str, object] = {
        "signature_present": False,
        "body_is_return_only": False,
        "logevent_call_in_body": True,  # default true → fail unless proven absent
        "tengu_literal_in_body": True,
        "callsite_count": 0,
        "logevent_import_present": False,  # must remain True (3 other callers)
        "analytics_metadata_import_present": False,  # must remain False
        "json_stringify_import_present": False,  # must remain False
    }

    # Find function definition body. Signature line is one of:
    #   function logClassifierResultForMossen(
    sig_re = re.compile(
        r"function logClassifierResultForMossen\(\s*"
        r"(?P<params>[^)]*)"
        r"\)\s*:\s*void\s*\{(?P<body>[\s\S]*?)\n\}",
        re.MULTILINE,
    )
    m = sig_re.search(text)
    if m is not None:
        findings["signature_present"] = True
        body = m.group("body")
        # Strip whitespace-only lines + leading/trailing whitespace
        stripped = "\n".join(
            line for line in body.splitlines() if line.strip()
        ).strip()
        # Body should be exactly: return
        findings["body_is_return_only"] = stripped == "return"
        findings["logevent_call_in_body"] = "logEvent(" in body
        findings["tengu_literal_in_body"] = (
            "tengu_internal_bash_classifier_result" in body
        )

    # Count occurrences of `logClassifierResultForMossen(` — should be 5:
    # 1 definition line + 4 callsites.
    findings["callsite_count"] = len(
        re.findall(r"logClassifierResultForMossen\(", text)
    )

    # Confirm dead imports were removed.
    findings["logevent_import_present"] = bool(
        re.search(r"^\s*logEvent,?\s*$", text, re.MULTILINE)
    ) or bool(re.search(r"\{\s*logEvent\s*\}", text))
    findings["analytics_metadata_import_present"] = (
        "AnalyticsMetadata_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS" in text
    )
    findings["json_stringify_import_present"] = bool(
        re.search(r"import\s+\{\s*jsonStringify\s*\}\s+from", text)
    )

    return findings


def main() -> int:
    failures: list[str] = []
    findings = static_assertion()

    if not findings["signature_present"]:
        failures.append(
            "logClassifierResultForMossen 函数签名未找到 — Wave 2A-A1 S2 不应删除签名 "
            "(签名保留留给 Wave 3 hard remove)"
        )
    else:
        if not findings["body_is_return_only"]:
            failures.append(
                "logClassifierResultForMossen 函数体不是单一 `return` "
                "(S2 要求函数体置空,仅保留 return)"
            )
        if findings["logevent_call_in_body"]:
            failures.append(
                "logClassifierResultForMossen 函数体内仍含 logEvent( 调用 "
                "(S2 要求移除 BQ 上报副作用)"
            )
        if findings["tengu_literal_in_body"]:
            failures.append(
                "logClassifierResultForMossen 函数体内仍含 "
                "'tengu_internal_bash_classifier_result' 字面量 (S2 不允许)"
            )

    if findings["callsite_count"] != 5:
        failures.append(
            f"logClassifierResultForMossen 引用计数 = {findings['callsite_count']},预期 5 "
            "(1 定义 + 4 callsites);S2 不应改 callsites"
        )

    # logEvent import MUST remain — file 另有 3 处合法 logEvent 调用
    # (lines 1700 tengu_tree_sitter_shadow / 1725 tengu_bash_ast_too_complex /
    #  2335 tengu_tree_sitter_security_divergence)
    if not findings["logevent_import_present"]:
        failures.append(
            "logEvent import 不应被删除 — 文件另有 3 处合法 logEvent 调用 "
            "(tengu_tree_sitter_shadow / tengu_bash_ast_too_complex / "
            "tengu_tree_sitter_security_divergence) 不属于 BQ 真命令上报范畴"
        )
    if findings["analytics_metadata_import_present"]:
        failures.append(
            "AnalyticsMetadata_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS import 未清理 "
            "(仅在被置空函数内被引用,应已成 dead import)"
        )
    if findings["json_stringify_import_present"]:
        failures.append(
            "jsonStringify import 未清理 — 仅在 logClassifierResultForMossen 体内使用,"
            "函数体置空后已成 dead import"
        )

    report = {
        "name": "wave2_a1_bash_bq_no_upload_smoke",
        "mode": "static-only",
        "mode_reason": (
            "tools/BashTool/bashPermissions.ts 透传依赖含 deferred runtime 模块 "
            "(REPLTool.js 等),`bun -e` 解析不到 source。S2 是纯结构改动,"
            "静态断言已足够;运行行为由 TUI 集成 smoke + full smoke 兜底。"
        ),
        "static_findings": findings,
        "failures": failures,
        "passed": 0 if failures else 1,
        "total": 1,
    }
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
