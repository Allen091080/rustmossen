#!/usr/bin/env python3
"""
audit_hardcoded_user_text — UX-Wave1 S8 产物。

目的：扫出 mossen 源码中"疑似硬编码英文用户文案"，作为后续 slice 逐步迁到
i18n 的 baseline 工具。

定位（W1-D9 = A）：
  - 这是 baseline-relative 的扫描器，不是"零硬编码"红线。
  - 第一次跑会把当前命中全部记录到 i18n_hardcoded_allowlist.txt。
  - 后续运行：命中数 > baseline → exit 1 (新增硬编码 = 回归)。
                命中数 ≤ baseline → exit 0 (允许减少；减少后用 --update-baseline 收紧)。

不要做（W1-D9 = A 明确范围限制）：
  - 不解析 AST（保持 Python 标准库，不引第三方）
  - 不试图替换、重写代码（只读）
  - 不报告 R-20 红线域内的命中（services/, utils/customBackend.ts,
    commands/status/, scripts/smoke_check.py）

扫描目标：
  - components/**/*.tsx, components/**/*.ts
  - commands/**/*.tsx, commands/**/*.ts
  - 不扫 *.test.* / *.spec.* / __tests__/

匹配模式（保守版，第一阶段精度优先于召回）：
  P1. description: '...'   命令注册表里的英文描述（≥8 字符且全英文）
  P2. hint: '...'          命令 hint
  P3. title: '...'         JSX prop title
  P4. placeholder: '...'   JSX prop placeholder
  P5. label: '...'         Select option label

排除：
  - 命中字符串包含中文 → 已本地化，跳过
  - 命中字符串短于 8 字符 → 噪声
  - 命中字符串是 url / path / 标识符（[a-z_-]+ 不带空格）
  - 行内出现 'getLocalizedText' / 't(' / 'i18n' → 已走 i18n 路径
  - 路径在 R-20 红线域内
  - 路径在 allowlist 内（fingerprint = path + ':' + line + ':' + sha256(snippet)[:12]）

Exit code:
  0  — 命中数 ≤ baseline，无回归
  1  — 命中数 > baseline，有新增硬编码 OR allowlist 文件缺失
  2  — 参数错误 / IO 异常

Usage:
  python3 scripts/audit_hardcoded_user_text.py
  python3 scripts/audit_hardcoded_user_text.py --update-baseline   # 重写 allowlist
  python3 scripts/audit_hardcoded_user_text.py --list              # 列出全部命中
"""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ALLOWLIST_PATH = REPO_ROOT / "scripts" / "i18n_hardcoded_allowlist.txt"

SCAN_DIRS = [REPO_ROOT / "components", REPO_ROOT / "commands"]
SCAN_EXTS = {".ts", ".tsx"}

# R-20 红线域：禁止报告（既不报命中也不进 baseline）
RED_LINE_PREFIXES = (
    "services/",
    "utils/customBackend.ts",
    "commands/status/",
    "scripts/smoke_check.py",
)

# 跳过测试文件
TEST_PATTERNS = (".test.", ".spec.", "__tests__/", "/test/", "/tests/")

# 已 i18n 化的标记 (行级排除)
I18N_HINTS = ("getLocalizedText(", "t(", "STRINGS_EN", "STRINGS_ZH", "i18n")

# 模式：prop_name: '...' 或 prop_name: "..."
# 捕获 group 1 = 字符串内容
PATTERN = re.compile(
    r"""\b(description|hint|title|placeholder|label)\s*:\s*['"]([^'"\n]{8,})['"]"""
)

# 中文字符
HAN = re.compile(r"[一-鿿]")

# url / path / 单纯标识符 (不含空格)
URL_OR_PATH = re.compile(r"^(https?://|/|\./|\.\./|[a-z][a-z0-9_./\-]*$)")
ALL_IDENTIFIER = re.compile(r"^[a-z][a-zA-Z0-9_-]*$")


@dataclass(frozen=True)
class Hit:
    rel_path: str
    line: int
    prop: str
    snippet: str

    def fingerprint(self) -> str:
        digest = hashlib.sha256(self.snippet.encode("utf-8")).hexdigest()[:12]
        return f"{self.rel_path}:{self.line}:{digest}"


def is_i18n_line(line: str) -> bool:
    return any(hint in line for hint in I18N_HINTS)


def should_skip_snippet(snippet: str) -> bool:
    if HAN.search(snippet):
        return True  # 含中文 → 已本地化
    if len(snippet.strip()) < 8:
        return True
    if " " not in snippet:
        # 没空格 → 大概率是标识符 / url / path
        if URL_OR_PATH.match(snippet) or ALL_IDENTIFIER.match(snippet):
            return True
    return False


def is_red_line(rel_path: str) -> bool:
    return any(rel_path.startswith(p) for p in RED_LINE_PREFIXES)


def is_test_file(rel_path: str) -> bool:
    return any(p in rel_path for p in TEST_PATTERNS)


def iter_files() -> list[Path]:
    files: list[Path] = []
    for scan_dir in SCAN_DIRS:
        if not scan_dir.exists():
            continue
        for path in scan_dir.rglob("*"):
            if path.suffix not in SCAN_EXTS:
                continue
            if not path.is_file():
                continue
            files.append(path)
    return files


def scan_file(path: Path) -> list[Hit]:
    rel_path = str(path.relative_to(REPO_ROOT))
    if is_red_line(rel_path) or is_test_file(rel_path):
        return []
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return []
    hits: list[Hit] = []
    for line_num, line in enumerate(text.splitlines(), start=1):
        if is_i18n_line(line):
            continue
        for m in PATTERN.finditer(line):
            prop, snippet = m.group(1), m.group(2)
            if should_skip_snippet(snippet):
                continue
            hits.append(
                Hit(rel_path=rel_path, line=line_num, prop=prop, snippet=snippet)
            )
    return hits


def load_allowlist() -> set[str]:
    if not ALLOWLIST_PATH.exists():
        return set()
    fingerprints: set[str] = set()
    for raw in ALLOWLIST_PATH.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        fingerprints.add(line)
    return fingerprints


def write_allowlist(hits: list[Hit]) -> None:
    lines = [
        "# UX-Wave1 S8 — i18n hardcoded baseline allowlist",
        "# 每行 = path:line:sha256(snippet)[:12]",
        "# 不要手编；用 audit_hardcoded_user_text.py --update-baseline 重写。",
        "",
    ]
    for fp in sorted({h.fingerprint() for h in hits}):
        lines.append(fp)
    ALLOWLIST_PATH.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--update-baseline",
        action="store_true",
        help="重写 allowlist 为当前命中集（由开发者主动收紧 baseline 时用）",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="列出全部命中（含已在 allowlist 中的），用于人工审阅",
    )
    args = parser.parse_args()

    all_hits: list[Hit] = []
    for path in iter_files():
        all_hits.extend(scan_file(path))

    if args.update_baseline:
        write_allowlist(all_hits)
        print(
            f"[audit_hardcoded] baseline updated: "
            f"{len({h.fingerprint() for h in all_hits})} fingerprints "
            f"({len(all_hits)} hits)"
        )
        return 0

    if not ALLOWLIST_PATH.exists():
        print(
            "[audit_hardcoded] FAIL: allowlist file missing. "
            "Run with --update-baseline once to seed it.",
            file=sys.stderr,
        )
        return 1

    baseline = load_allowlist()
    new_hits = [h for h in all_hits if h.fingerprint() not in baseline]
    obsolete = baseline - {h.fingerprint() for h in all_hits}

    if args.list:
        print(f"[audit_hardcoded] total hits = {len(all_hits)}; baseline = "
              f"{len(baseline)}; new = {len(new_hits)}; obsolete = {len(obsolete)}")
        for h in sorted(all_hits, key=lambda x: (x.rel_path, x.line)):
            in_baseline = h.fingerprint() in baseline
            tag = "BASELINE" if in_baseline else "NEW     "
            print(f"  {tag} {h.rel_path}:{h.line} [{h.prop}] {h.snippet!r}")
        return 0 if not new_hits else 1

    if new_hits:
        print(
            f"[audit_hardcoded] FAIL: {len(new_hits)} new hardcoded "
            f"user-facing string(s) detected (not in allowlist):",
            file=sys.stderr,
        )
        for h in sorted(new_hits, key=lambda x: (x.rel_path, x.line)):
            print(
                f"  - {h.rel_path}:{h.line} [{h.prop}] {h.snippet!r}",
                file=sys.stderr,
            )
        print(
            "\nFix options:",
            "\n  1. Migrate the string to t() via utils/i18n.",
            "\n  2. If intentionally untranslated, run with --update-baseline "
            "and explain in commit message.",
            file=sys.stderr,
        )
        return 1

    print(
        f"[audit_hardcoded] OK: {len(all_hits)} hits, all in baseline "
        f"({len(obsolete)} obsolete entries can be cleaned with --update-baseline)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
