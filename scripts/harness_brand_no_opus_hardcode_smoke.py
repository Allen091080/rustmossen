#!/usr/bin/env python3
"""
Brand guard: components/EffortCallout.tsx 不应硬编码 'opus' 模型名。

Wave 1 MUST-FIX-009 把 shouldShowEffortCallout 内 'opus-4-6' 硬编码降级为 return false,
理由是 Mossen 的 callout 应基于自有 profile/capability metadata, 不应依赖供应商模型名字符串。
等 Mossen capability 系统落地后,再实现 capability-based gating
(见 审计结果/needs-design-决策表.md)。

本 smoke 防止后续 PR 顺手加回 'opus-' 字面量,保持品牌脱钩。
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "components" / "EffortCallout.tsx"


def main() -> int:
    text = TARGET.read_text(encoding="utf-8")
    # Forbid any quoted 'opus' literal in code logic (e.g. includes('opus'), 'opus-4-6', "opus-").
    # Comments are checked via the same patterns to prevent silent re-introduction.
    forbidden_patterns = [
        "opus-4-6",
        "'opus-'",
        '"opus-"',
        "'opus'",
        '"opus"',
        "includes('opus",
        'includes("opus',
    ]
    hits = []
    for line_num, line in enumerate(text.splitlines(), start=1):
        for pattern in forbidden_patterns:
            if pattern in line:
                hits.append(
                    {
                        "line": line_num,
                        "pattern": pattern,
                        "snippet": line.strip()[:200],
                    }
                )
    result = {
        "ok": len(hits) == 0,
        "target": str(TARGET.relative_to(ROOT)),
        "forbidden_patterns": forbidden_patterns,
        "hits": hits,
        "design_note": (
            "EffortCallout 必须不硬编码任何 'opus' 模型名 (Wave 1 MUST-FIX-009)。"
            "未来 capability metadata 落地后, callout 应基于 profile.supportsEffort / "
            "recommendedEffort 等内部能力字段, 不依赖任何供应商模型 id。"
        ),
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
