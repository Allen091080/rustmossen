#!/usr/bin/env python3
"""CWB-3 — Mossen stream-json protocol contract audit smoke (静态校验).

校验 10 项 (100% 静态, 0 network / 0 LLM / 0 mossen 启动. 目标 < 2s):

  A. whitelist 文件存在且非空
  B. coreSchemas.ts SDKMessage union 成员实测集 == whitelist Section A
  C. controlSchemas.ts SDKControlRequestInner union 21 成员实测集 == whitelist Section B
  D. controlSchemas.ts StdoutMessage union 8 成员实测集 == whitelist Section C
  E. controlSchemas.ts StdinMessage union 5 成员实测集 == whitelist Section D
  F. main.tsx 含 stream-json 入口锚点 + 双端一致校验 (whitelist Section E)
  G. cli/print.ts 含 runHeadless / getStructuredIO / verbose 守卫 (whitelist Section E)
  H. cli/structuredIO.ts 含 StructuredIO class + control_request + keep_alive (whitelist Section E)
  I. cli/ndjsonSafeStringify.ts 仍含唯一 ndjsonSafeStringify export (whitelist Section E)
  J. entrypoints/sdk/coreTypes.ts 仍含 known blocker `./sdkUtilityTypes.js` (CWB-D9 推延依据)

设计原则:
  - 漂移即 fail (任何 schema 增删 / 重命名 / 入口破坏都会被发现)
  - smoke 失败时只报告, 不擅自改源码 (red-lines.md §5)
  - 维护: schema 改动 → 同 commit 改 whitelist + protocol-contract.md + 本 smoke 锚点

退出码:
  0 = PASS
  1 = FAIL (列出失败项)
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WHITELIST_FILE = ROOT / "scripts" / "stream-json-schema-whitelist.txt"
CORE_SCHEMAS = ROOT / "entrypoints" / "sdk" / "coreSchemas.ts"
CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
MAIN_TSX = ROOT / "main.tsx"
PRINT_TS = ROOT / "cli" / "print.ts"
STRUCTURED_IO_TS = ROOT / "cli" / "structuredIO.ts"
NDJSON_SAFE_TS = ROOT / "cli" / "ndjsonSafeStringify.ts"
SDK_CORE_TYPES = ROOT / "entrypoints" / "sdk" / "coreTypes.ts"

SCHEMA_NAME_RE = re.compile(r"^\s*(SDK[A-Z][A-Za-z0-9]*Schema)\(\)\s*,?$")


def load_whitelist() -> dict[str, list[str]]:
    if not WHITELIST_FILE.exists():
        return {}
    sections: dict[str, list[str]] = {
        "A": [],
        "B": [],
        "C": [],
        "D": [],
        "E": [],
        "F": [],
    }
    current: str | None = None
    for line in WHITELIST_FILE.read_text(encoding="utf-8").splitlines():
        if line.startswith("# Section A"):
            current = "A"
            continue
        if line.startswith("# Section B"):
            current = "B"
            continue
        if line.startswith("# Section C"):
            current = "C"
            continue
        if line.startswith("# Section D"):
            current = "D"
            continue
        if line.startswith("# Section E"):
            current = "E"
            continue
        if line.startswith("# Section F"):
            current = "F"
            continue
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if current is None:
            continue
        sections[current].append(stripped)
    return sections


def extract_union_members(path: Path, union_export_name: str) -> list[str]:
    """Extract Schema names from a `z.union([...])` block right after `export const X = lazySchema(() => z.union([...]))`."""
    if not path.exists():
        return []
    text = path.read_text(encoding="utf-8")
    pattern = re.compile(
        rf"export const {re.escape(union_export_name)}\s*=\s*lazySchema\(\(\)\s*=>\s*\n?\s*z\.union\(\[(.*?)\]\),?\s*\n?\)",
        re.DOTALL,
    )
    m = pattern.search(text)
    if not m:
        return []
    body = m.group(1)
    members: list[str] = []
    for line in body.splitlines():
        m2 = SCHEMA_NAME_RE.match(line)
        if m2:
            members.append(m2.group(1))
    return members


def file_contains(path: Path, needle: str) -> bool:
    if not path.exists():
        return False
    return needle in path.read_text(encoding="utf-8")


def check_a_whitelist_nonempty(sections: dict[str, list[str]]) -> list[str]:
    if not WHITELIST_FILE.exists():
        return [f"whitelist missing: {WHITELIST_FILE}"]
    if not any(sections.values()):
        return ["whitelist is empty (no entries in any section)"]
    return []


def check_union_match(
    label: str,
    actual: list[str],
    whitelist: list[str],
    expected_count: int,
) -> list[str]:
    errors: list[str] = []
    actual_set = set(actual)
    whitelist_set = {item for item in whitelist if not item.startswith("StdoutMessage:") and not item.startswith("StdinMessage:")}
    # Section C/D entries have prefix `StdoutMessage:` / `StdinMessage:`; strip
    if not whitelist_set:
        whitelist_set = {item.split(":", 1)[1] for item in whitelist if ":" in item}

    if len(actual) != expected_count:
        errors.append(
            f"{label}: 实测 {len(actual)} 成员, 期望 {expected_count} (whitelist 同步是否漏了?)"
        )
    extra_in_actual = actual_set - whitelist_set
    extra_in_whitelist = whitelist_set - actual_set
    if extra_in_actual:
        errors.append(
            f"{label}: 源码新增未入 whitelist: {sorted(extra_in_actual)} "
            f"— 同 commit 改 scripts/stream-json-schema-whitelist.txt + docs/reference/protocol-contract.md"
        )
    if extra_in_whitelist:
        errors.append(
            f"{label}: whitelist 列出但源码 0 实测: {sorted(extra_in_whitelist)} "
            f"— 源码删除/重命名? 触 only-additive 红线 (red-lines.md §3)"
        )
    return errors


def check_anchors(section_e: list[str]) -> list[str]:
    """Section E entries have form `anchor:<file>:<needle>` or `known_blocker:<file>:<needle>`."""
    errors: list[str] = []
    for entry in section_e:
        if not entry.startswith("anchor:"):
            continue
        try:
            _, file_part, needle = entry.split(":", 2)
        except ValueError:
            errors.append(f"malformed anchor entry: {entry}")
            continue
        target = ROOT / file_part
        if not target.exists():
            errors.append(f"anchor file missing: {file_part}")
            continue
        if not file_contains(target, needle):
            errors.append(
                f"anchor missing in {file_part}: '{needle}' "
                f"— stream-json 入口/守卫被破坏? 触 only-additive 红线"
            )
    return errors


def check_known_blockers(section_f: list[str]) -> list[str]:
    """Section F entries assert known blockers still present (CWB-D9 推延依据)."""
    errors: list[str] = []
    for entry in section_f:
        if not entry.startswith("known_blocker:"):
            continue
        try:
            _, file_part, needle = entry.split(":", 2)
        except ValueError:
            errors.append(f"malformed known_blocker entry: {entry}")
            continue
        target = ROOT / file_part
        if not target.exists():
            errors.append(f"known_blocker file missing: {file_part}")
            continue
        if not file_contains(target, needle):
            errors.append(
                f"known_blocker resolved without notice in {file_part}: '{needle}' "
                f"— SDK in-process 路径已修? 必须同 commit 删 known_blocker 行 + 更新 CWB-D9 状态"
            )
    return errors


def main() -> int:
    sections = load_whitelist()

    sdk_message_union = extract_union_members(CORE_SCHEMAS, "SDKMessageSchema")
    control_inner_union = extract_union_members(
        CONTROL_SCHEMAS, "SDKControlRequestInnerSchema"
    )
    stdout_union = extract_union_members(CONTROL_SCHEMAS, "StdoutMessageSchema")
    stdin_union = extract_union_members(CONTROL_SCHEMAS, "StdinMessageSchema")

    print("=== CWB-3 stream-json contract audit ===")
    print(f"whitelist sections (A-F): "
          f"{[len(sections.get(s, [])) for s in ['A','B','C','D','E','F']]}")
    print(f"SDKMessage union (实测)        : {len(sdk_message_union)}")
    print(f"SDKControlRequestInner (实测)  : {len(control_inner_union)}")
    print(f"StdoutMessage union (实测)     : {len(stdout_union)}")
    print(f"StdinMessage union (实测)      : {len(stdin_union)}")

    failures: list[str] = []
    failures += check_a_whitelist_nonempty(sections)
    failures += check_union_match(
        "B (SDKMessage)", sdk_message_union, sections.get("A", []), 28
    )
    failures += check_union_match(
        "C (SDKControlRequestInner)",
        control_inner_union,
        sections.get("B", []),
        29,
    )
    failures += check_union_match(
        "D (StdoutMessage)", stdout_union, sections.get("C", []), 8
    )
    failures += check_union_match(
        "E (StdinMessage)", stdin_union, sections.get("D", []), 6
    )
    failures += check_anchors(sections.get("E", []))
    failures += check_known_blockers(sections.get("F", []))

    if failures:
        print()
        print("=== FAIL ===")
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: subprocess stream-json contract ✓ "
        "(28 SDKMessage + 29 control + 8 stdout + 6 stdin + anchors + known blockers)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
