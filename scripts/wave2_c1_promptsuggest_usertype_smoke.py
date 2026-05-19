#!/usr/bin/env python3
"""Wave 2C — C1 (TYPE-MIGRATE-PROMPTSUGGEST) focused smoke (static-only).

Verifies single-token swap in services/PromptSuggestion/promptSuggestion.ts:
  process.env.USER_TYPE === 'external' → getUserType() === 'external'

让 mossen 默认 (USER_TYPE=undefined) 走 'external' fallback,杜绝绕过 rate-limit
抑制 (与 Wave 0 API-001 同思路)。

Why static-only:
  * promptSuggestion.ts 透传依赖含 deferred 模块 — runtime exercise belongs
    in TUI integration smoke.
  * 单 token swap 是纯结构改动, 静态断言已足够。
  * 真实 rate-limit 抑制行为由 TUI smoke 兜底。

SA-2 已确认 `=== 'external'` 全仓仅 2 处:
  - 1 已 Wave 0 API-001 迁 (utils/api/withRetry.ts)
  - 1 待迁 = 本 slice (promptSuggestion.ts:114)
本 slice 收敛为单点。
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "services" / "PromptSuggestion" / "promptSuggestion.ts"


def static_assertion() -> dict[str, object]:
    findings: dict[str, object] = {
        "getUserType_imported": False,
        "rate_limit_uses_getUserType": False,
        "rate_limit_no_direct_env": False,
        "external_eq_count_in_file": 0,
    }

    text = TARGET.read_text(encoding="utf-8")

    # 1. import getUserType from userType
    if re.search(
        r"import\s+\{\s*getUserType\s*\}\s+from\s+['\"][^'\"]*userType",
        text,
    ):
        findings["getUserType_imported"] = True

    # 2. Find rate_limit if-block (allow whitespace/newlines, balanced parens).
    # Locate `return 'rate_limit'` and walk back to its `if (`.
    rate_idx = text.find("return 'rate_limit'")
    if rate_idx >= 0:
        # Find the most recent `if (` before this return.
        if_idx = text.rfind("if (", 0, rate_idx)
        if if_idx >= 0:
            cond = text[if_idx + 4 : rate_idx]
            if re.search(r"\bgetUserType\(\)\s*===\s*'external'", cond):
                findings["rate_limit_uses_getUserType"] = True
            if "process.env.USER_TYPE" not in cond:
                findings["rate_limit_no_direct_env"] = True

    # 4. Count `=== 'external'` total occurrences in this file (sanity).
    findings["external_eq_count_in_file"] = len(
        re.findall(r"===\s*'external'", text)
    )

    return findings


def main() -> int:
    failures: list[str] = []
    f = static_assertion()

    if not f["getUserType_imported"]:
        failures.append(
            "getUserType import 缺失 — C1 要求从 utils/userType.js import"
        )
    if not f["rate_limit_uses_getUserType"]:
        failures.append(
            "rate_limit if-block 内未使用 getUserType() === 'external'"
        )
    if not f["rate_limit_no_direct_env"]:
        failures.append(
            "rate_limit if-block 内仍直接读 process.env.USER_TYPE — C1 要求消除"
        )
    if f["external_eq_count_in_file"] != 1:
        failures.append(
            f"=== 'external' 在该文件出现 {f['external_eq_count_in_file']} 次,"
            "预期 1 (仅 rate_limit if-block)"
        )

    report = {
        "name": "wave2_c1_promptsuggest_usertype_smoke",
        "mode": "static-only",
        "mode_reason": (
            "promptSuggestion.ts 透传依赖含 deferred 模块。单 token swap 是纯结构, "
            "静态断言已足够;真实 rate-limit 抑制由 TUI smoke 兜底。"
        ),
        "static_findings": f,
        "failures": failures,
        "passed": 4 - len(failures),
        "total": 4,
    }
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
