#!/usr/bin/env python3
"""Wave 4 R8.2 — Bun feature flag 体系 audit smoke (静态校验).

校验 4 项:
  A. bunfig.toml [define] 段所有 key 必须以 `MACRO.` 前缀 (不许混入 feature gate)
  B. 全仓 feature('TOKEN') 调用收集到的唯一 token 集 == scripts/feature-flag-token-whitelist.txt
     新增 token 或缺失 token 都 fail (并提示同步白名单 + design doc §2.2)
  C. platform/featureGatesRuntime.ts 内 resolve('TOKEN') 调用的 token 必须满足:
        在白名单内 (有 feature() callsite)  OR
        在 KNOWN_DEBT_RESOLVE_ORPHANS (Wave 5 Phase 2 后清空)
     其他 orphan resolve token = dead code, fail
  D. (输出诊断) 列出 known debt 当前清单, 供 Allen 跟踪

设计:
  - 100% 静态: 0 network / 0 LLM / 0 mossen 启动
  - 目标 < 3s
  - 不读 .mossensrc/feature-flags.env (per-developer, 不入仓)
  - 不动 bunfig.toml (只读校验)

退出码:
  0 = PASS (含 known debt 输出, 不阻断)
  1 = FAIL (列出失败项)

维护:
  - 新增 feature gate → 同 commit 加 token 到 scripts/feature-flag-token-whitelist.txt
  - 移除 feature gate → 同 commit 删 token, 同步 design doc §2.2
  - 新发现 orphan resolve token → Allen 拍板加入 KNOWN_DEBT_RESOLVE_ORPHANS 或清除 dead code
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BUNFIG = ROOT / "bunfig.toml"
FEATURE_GATES_RUNTIME = ROOT / "platform" / "featureGatesRuntime.ts"
WHITELIST_FILE = ROOT / "scripts" / "feature-flag-token-whitelist.txt"

# Wave 5 Phase 2 已清空 BRIDGE_MODE resolve orphan (Bridge 子系统 Wave 1.5 删除尾巴).
# 任何新增 orphan 必须 Allen 拍板加入此集合或清除 dead code.
KNOWN_DEBT_RESOLVE_ORPHANS: frozenset[str] = frozenset()

FEATURE_CALLSITE_RE = re.compile(r"feature\(['\"]([A-Z_][A-Z0-9_]*)['\"]")
RESOLVE_CALLSITE_RE = re.compile(r"resolve\(['\"]([A-Z_][A-Z0-9_]*)['\"]")
DEFINE_KEY_RE = re.compile(r'^"([^"]+)"\s*=')

SOURCE_SUFFIXES = {".ts", ".tsx"}
SKIP_DIRS = {"node_modules", ".git", "dist", "build", ".cache", ".turbo"}


def load_whitelist() -> set[str]:
    if not WHITELIST_FILE.exists():
        sys.stderr.write(f"FAIL: whitelist file missing: {WHITELIST_FILE}\n")
        raise SystemExit(1)
    return {
        line.strip()
        for line in WHITELIST_FILE.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    }


def collect_feature_tokens() -> set[str]:
    tokens: set[str] = set()
    for path in ROOT.rglob("*"):
        if path.suffix not in SOURCE_SUFFIXES:
            continue
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        tokens.update(FEATURE_CALLSITE_RE.findall(text))
    return tokens


def collect_resolve_tokens() -> set[str]:
    if not FEATURE_GATES_RUNTIME.exists():
        return set()
    text = FEATURE_GATES_RUNTIME.read_text(encoding="utf-8")
    return set(RESOLVE_CALLSITE_RE.findall(text))


def check_a_bunfig_macro_only() -> list[str]:
    if not BUNFIG.exists():
        return [f"bunfig.toml missing at {BUNFIG}"]
    errors: list[str] = []
    in_define = False
    for line in BUNFIG.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if stripped == "[define]":
            in_define = True
            continue
        if stripped.startswith("[") and stripped.endswith("]") and stripped != "[define]":
            in_define = False
            continue
        if not in_define or not stripped or stripped.startswith("#"):
            continue
        m = DEFINE_KEY_RE.match(stripped)
        if m and not m.group(1).startswith("MACRO."):
            errors.append(
                f"bunfig.toml [define] non-MACRO key: {m.group(1)} "
                f"(feature gate should use feature() API, not bunfig [define])"
            )
    return errors


def check_b_whitelist(actual: set[str], whitelist: set[str]) -> tuple[set[str], set[str]]:
    unknown = actual - whitelist
    missing = whitelist - actual
    return unknown, missing


def check_c_resolve(resolve_tokens: set[str], whitelist: set[str]) -> tuple[set[str], set[str]]:
    in_whitelist = resolve_tokens & whitelist
    orphans = resolve_tokens - whitelist - KNOWN_DEBT_RESOLVE_ORPHANS
    known_debt = resolve_tokens & KNOWN_DEBT_RESOLVE_ORPHANS
    return orphans, known_debt


def main() -> int:
    whitelist = load_whitelist()
    feature_tokens = collect_feature_tokens()
    resolve_tokens = collect_resolve_tokens()

    a_errors = check_a_bunfig_macro_only()
    unknown, missing = check_b_whitelist(feature_tokens, whitelist)
    orphans, known_debt = check_c_resolve(resolve_tokens, whitelist)

    print("=== Wave 4 R8.2: Bun Feature Flag Audit ===")
    print(f"feature() unique tokens (实测) : {len(feature_tokens)}")
    print(f"whitelist tokens               : {len(whitelist)}")
    print(f"resolve() tokens               : {len(resolve_tokens)} "
          f"(in featureGatesRuntime.ts)")
    print(f"resolve in whitelist           : "
          f"{len(resolve_tokens & whitelist)}")
    print(f"unknown feature tokens         : {sorted(unknown) or '(none)'}")
    print(f"missing whitelist tokens       : {sorted(missing) or '(none)'}")
    print(f"resolve-only orphans (FAIL)    : {sorted(orphans) or '(none)'}")
    print(f"known debt (Wave 5+ cleanup)   : {sorted(known_debt) or '(none)'}")

    failures: list[str] = []
    failures.extend(a_errors)
    if unknown:
        failures.append(
            f"feature() tokens not in whitelist: {sorted(unknown)} "
            f"— add to scripts/feature-flag-token-whitelist.txt + "
            f"docs/design/bun-feature-flag-system.md §2.2"
        )
    if missing:
        failures.append(
            f"whitelist tokens with 0 feature() callsite: {sorted(missing)} "
            f"— remove from scripts/feature-flag-token-whitelist.txt + "
            f"docs/design/bun-feature-flag-system.md §2.2"
        )
    if orphans:
        failures.append(
            f"resolve('TOKEN') in featureGatesRuntime.ts with 0 feature() "
            f"callsite and not in KNOWN_DEBT_RESOLVE_ORPHANS: {sorted(orphans)} "
            f"— either add feature() callsite, mark as known debt, or remove "
            f"the resolve()/snapshot field"
        )

    if failures:
        print()
        print("=== FAIL ===")
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print()
    print("PASS: bunfig MACRO ✓ / feature whitelist sync ✓ / resolve consistency ✓")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
