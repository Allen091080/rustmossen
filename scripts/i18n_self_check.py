#!/usr/bin/env python3
"""
i18n_self_check — UX-Wave1 S1 产物。

校验：
  1. utils/i18n/strings.en.ts 与 strings.zh.ts 的 key 集合严格相等。
  2. 每个 key 满足命名公约 (W1-D5 = A): <scope>.<feature>.<element>，三级。
     scope ∈ {cmd, ui, ctx, compact, onboarding, hosted, lang, statusline, spinner}
  3. 不存在 key 在 en 是空字符串 (避免误把空字符串当占位)。

Exit code:
  0  — 一切正常。
  1  — 任一项失败；stderr 列具体差异。

不读取 runtime 的 t()，纯静态分析；不依赖 mossen 启动；不动用户 ~/.mossen。

使用：
  python3 scripts/i18n_self_check.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EN_PATH = REPO_ROOT / "utils" / "i18n" / "strings.en.ts"
ZH_PATH = REPO_ROOT / "utils" / "i18n" / "strings.zh.ts"

ALLOWED_SCOPES = {
    "cmd",
    "ui",
    "ctx",
    "compact",
    "onboarding",
    "hosted",
    "lang",
    "statusline",
    "spinner",
}

# 匹配形如:  'scope.feature.element': 'value'    或   'scope.feature.element': "value"
# 仅捕获 key (双引号 / 单引号都允许)。值不解析。
KEY_LINE = re.compile(
    r"""^\s*['"]([a-z][a-z0-9-]*\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)['"]\s*:\s*['"]""",
    re.MULTILINE,
)

# 用于 key 命名公约校验
KEY_PATTERN = re.compile(
    r"^([a-z][a-z0-9-]*)\.([A-Za-z][A-Za-z0-9_-]*)\.([A-Za-z][A-Za-z0-9_-]*)$"
)

# 用于 en 值是否空字符串
EMPTY_VALUE_LINE = re.compile(
    r"""^\s*['"]([a-z][A-Za-z0-9_.-]+)['"]\s*:\s*['"]['"]\s*,?\s*$""",
    re.MULTILINE,
)


def parse_keys(path: Path) -> list[str]:
    if not path.exists():
        print(f"[i18n_self_check] missing file: {path}", file=sys.stderr)
        sys.exit(1)
    text = path.read_text(encoding="utf-8")
    return KEY_LINE.findall(text)


def parse_empty_values(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    return EMPTY_VALUE_LINE.findall(text)


def main() -> int:
    en_keys = parse_keys(EN_PATH)
    zh_keys = parse_keys(ZH_PATH)

    failures: list[str] = []

    # 1. 重复 key 检查（同表内）
    en_dup = sorted({k for k in en_keys if en_keys.count(k) > 1})
    zh_dup = sorted({k for k in zh_keys if zh_keys.count(k) > 1})
    if en_dup:
        failures.append(f"strings.en.ts duplicate keys: {en_dup}")
    if zh_dup:
        failures.append(f"strings.zh.ts duplicate keys: {zh_dup}")

    en_set = set(en_keys)
    zh_set = set(zh_keys)

    # 2. 对称性
    missing_in_zh = sorted(en_set - zh_set)
    missing_in_en = sorted(zh_set - en_set)
    if missing_in_zh:
        failures.append(f"missing in strings.zh.ts: {missing_in_zh}")
    if missing_in_en:
        failures.append(f"missing in strings.en.ts: {missing_in_en}")

    # 3. 命名公约
    bad_named: list[str] = []
    bad_scope: list[str] = []
    for k in sorted(en_set | zh_set):
        m = KEY_PATTERN.match(k)
        if not m:
            bad_named.append(k)
            continue
        scope = m.group(1)
        if scope not in ALLOWED_SCOPES:
            bad_scope.append(f"{k} (scope='{scope}')")
    if bad_named:
        failures.append(
            "keys not matching <scope>.<feature>.<element>: " + str(bad_named)
        )
    if bad_scope:
        failures.append(
            f"keys with disallowed scope (allowed: {sorted(ALLOWED_SCOPES)}): "
            + str(bad_scope)
        )

    # 4. 空字符串值（en）
    en_empty = parse_empty_values(EN_PATH)
    if en_empty:
        failures.append(f"strings.en.ts has empty-string values for keys: {en_empty}")

    if failures:
        print("[i18n_self_check] FAIL", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(
        f"[i18n_self_check] OK: {len(en_set)} keys, en/zh symmetric, "
        f"naming convention valid"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
