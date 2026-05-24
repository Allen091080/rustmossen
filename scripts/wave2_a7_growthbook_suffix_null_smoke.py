#!/usr/bin/env python3
"""Wave2A-A7 focused smoke: getInternalModelOverrideSection 静态封口验证。

3 case 全 PASS 才视为通过:
- case 1: getInternalModelOverrideSection 函数体首行非注释行就是 `return null`
- case 2: 函数内不含 GrowthBook / feature 字面量
- case 3: getInternalModelOverrideConfig (大写 Config) 函数仍存在 (依赖保留)
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROMPTS = ROOT / "constants" / "prompts.ts"
INTERNALMODELS = ROOT / "utils" / "model" / "internalModels.ts"


def extract_function_body(src: str, fname: str) -> str | None:
    """Extract { ... } body of a top-level `function fname(...)` definition."""
    pattern = re.compile(
        r"^function\s+" + re.escape(fname) + r"\s*\([^)]*\)\s*(?::[^\{]+)?\{",
        re.MULTILINE,
    )
    m = pattern.search(src)
    if not m:
        return None
    # Walk braces from m.end()-1 (the '{')
    i = m.end() - 1
    depth = 0
    start = i + 1
    while i < len(src):
        ch = src[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return src[start:i]
        i += 1
    return None


def case1_first_non_comment_is_return_null(body: str) -> tuple[bool, str]:
    for raw in body.splitlines():
        line = raw.strip()
        if not line:
            continue
        if line.startswith("//") or line.startswith("/*") or line.startswith("*"):
            continue
        ok = line == "return null" or line.startswith("return null")
        return ok, f"first non-comment line: {line!r}"
    return False, "no non-comment line found in function body"


def _strip_comments(body: str) -> str:
    """Remove // line comments and /* block comments */ to scan only code."""
    no_block = re.sub(r"/\*.*?\*/", "", body, flags=re.DOTALL)
    out_lines: list[str] = []
    for raw in no_block.splitlines():
        stripped = raw.lstrip()
        if stripped.startswith("//"):
            continue
        # Drop trailing // comment portion if present (naive but adequate here).
        idx = raw.find("//")
        if idx >= 0:
            raw = raw[:idx]
        out_lines.append(raw)
    return "\n".join(out_lines)


def case2_no_growthbook_literals(body: str) -> tuple[bool, str]:
    forbidden = [
        "feature(",
        "getFeatureValue",
        "growthbook",
        "GrowthBook",
        "KAIROS",
        "EXPERIMENTAL",
    ]
    code_only = _strip_comments(body)
    hits = [token for token in forbidden if token in code_only]
    if hits:
        return False, f"forbidden tokens present in code: {hits}"
    return True, "no GrowthBook / feature literals in code (comments ignored)"


def case3_config_function_preserved() -> tuple[bool, str]:
    if not INTERNALMODELS.exists():
        return False, f"internalModels.ts not found at {INTERNALMODELS}"
    src = INTERNALMODELS.read_text(encoding="utf-8")
    has_alias = "getInternalModelOverrideConfig" in src
    has_def = "getInternalModelOverrideConfig" in src
    if has_alias and has_def:
        return True, "getInternalModelOverrideConfig (alias) + getInternalModelOverrideConfig present"
    return False, f"missing — alias={has_alias} def={has_def}"


def main() -> int:
    if not PROMPTS.exists():
        print(f"FAIL: {PROMPTS} not found", file=sys.stderr)
        return 1
    src = PROMPTS.read_text(encoding="utf-8")
    body = extract_function_body(src, "getInternalModelOverrideSection")
    if body is None:
        print("FAIL: getInternalModelOverrideSection function not found in prompts.ts", file=sys.stderr)
        return 1

    results: list[tuple[str, bool, str]] = []
    ok, msg = case1_first_non_comment_is_return_null(body)
    results.append(("case1 first-non-comment-line == return null", ok, msg))
    ok, msg = case2_no_growthbook_literals(body)
    results.append(("case2 no GrowthBook/feature literals", ok, msg))
    ok, msg = case3_config_function_preserved()
    results.append(("case3 getInternalModelOverrideConfig preserved", ok, msg))

    passed = sum(1 for _, ok, _ in results if ok)
    total = len(results)
    for name, ok, msg in results:
        tag = "PASS" if ok else "FAIL"
        print(f"[{tag}] {name} — {msg}")
    print(f"\nresult: {passed}/{total} PASS")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
