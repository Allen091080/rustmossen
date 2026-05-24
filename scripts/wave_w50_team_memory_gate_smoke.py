#!/usr/bin/env python3
"""
W50 — team memory runtime gate smoke.

Verifies the current Rust team-memory gate remains independent from the KAIROS
gate and reaches the extraction/runtime paths that consume it.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MEMDIR = ROOT / "crates" / "mossen-cli" / "src" / "memdir.rs"
INTERACTIVE = ROOT / "crates" / "mossen-cli" / "src" / "interactive.rs"
ALIAS_MAP = ROOT / "crates" / "mossen-agent" / "src" / "services" / "config" / "alias_map.rs"
DEFAULTS = ROOT / "crates" / "mossen-agent" / "src" / "services" / "config" / "defaults.rs"
EXTRACT = ROOT / "crates" / "mossen-agent" / "src" / "services" / "extract_memories" / "mod.rs"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def read(path: Path) -> str:
    if not path.exists():
        raise FileNotFoundError(f"required source file is missing: {path.relative_to(ROOT)}")
    return path.read_text()


def function_body(src: str, signature: str) -> str:
    start = src.find(signature)
    if start < 0:
        return ""
    brace = src.find("{", start)
    if brace < 0:
        return ""
    depth = 0
    for i in range(brace, len(src)):
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
            if depth == 0:
                return src[start : i + 1]
    return src[start:]


def strip_line_comments(src: str) -> str:
    return "\n".join(
        line for line in src.splitlines() if not line.strip().startswith("//")
    )


def check_memdir_gate(failures: list[str]) -> None:
    src = read(MEMDIR)
    enabled = function_body(src, "pub fn is_team_memory_enabled() -> bool")
    rollout = function_body(src, "pub fn is_team_memory_rollout_enabled() -> bool")
    auto = function_body(src, "pub fn is_auto_memory_enabled() -> bool")

    if not enabled:
        fail(failures, "memdir.rs 缺 is_team_memory_enabled()")
    elif "is_auto_memory_enabled() && is_team_memory_rollout_enabled()" not in enabled:
        fail(failures, "is_team_memory_enabled() 必须由 auto-memory 与 team rollout 共同决定")

    for token in [
        "MOSSEN_CODE_DISABLE_TEAM_MEMORY",
        "MOSSEN_CODE_ENABLE_TEAM_MEMORY",
        "MOSSEN_TEAM_MEMORY",
        "MOSSEN_MEMORY_TEAM_MEMORY_ENABLED",
        "MOSSEN_TEAM_MEMORY_ENABLED",
        "team_memory_sync::is_team_memory_sync_available()",
    ]:
        if token not in rollout:
            fail(failures, f"is_team_memory_rollout_enabled() 缺 gate 来源: {token}")

    code = strip_line_comments(enabled + "\n" + rollout)
    if "mossen_herring_clock" in code or "MOSSEN_HERRING_CLOCK" in code:
        fail(failures, "team-memory gate 不得使用 KAIROS/herring clock gate")
    if auto and ("TEAM_MEMORY" in auto or "team_memory" in auto):
        fail(failures, "is_auto_memory_enabled() 不应依赖 team-memory gate")


def check_config_alias_and_defaults(failures: list[str]) -> None:
    alias_map = read(ALIAS_MAP)
    defaults = read(DEFAULTS)

    if 'm.insert("mossen_team_memory", "mossen.memory.teamMemoryEnabled")' not in alias_map:
        fail(failures, "alias_map.rs 缺 mossen_team_memory -> mossen.memory.teamMemoryEnabled")
    if 'm.insert("mossen_herring_clock", "mossen.memory.kairosActive")' not in alias_map:
        fail(failures, "alias_map.rs 必须保留 KAIROS mossen_herring_clock 映射")
    if 'm.insert("mossen.memory.teamMemoryEnabled", json!(false))' not in defaults:
        fail(failures, "defaults.rs 缺 mossen.memory.teamMemoryEnabled 默认 false")
    if 'm.insert("mossen.memory.kairosActive", json!(false))' not in defaults:
        fail(failures, "defaults.rs 必须保留 mossen.memory.kairosActive 默认 false")


def check_runtime_snapshot(failures: list[str]) -> None:
    src = read(INTERACTIVE)
    body = function_body(
        src,
        "pub async fn get_team_memory_runtime_snapshot() -> crate::platform::TeamMemoryRuntimeSnapshot",
    )
    for token in [
        "crate::memdir::is_team_memory_rollout_enabled()",
        "crate::memdir::is_team_memory_enabled()",
        "team_memory_sync::is_team_memory_sync_available()",
        "build_enabled: true",
        "rollout_enabled",
        "sync_available",
    ]:
        if token not in body:
            fail(failures, f"team-memory runtime snapshot 缺真实状态字段: {token}")
    if "mossen_herring_clock" in strip_line_comments(body):
        fail(failures, "team-memory runtime snapshot 不得使用 KAIROS/herring clock gate")


def check_extraction_prompt_gate(failures: list[str]) -> None:
    src = read(EXTRACT)
    if "pub team_memory_enabled: bool" not in src:
        fail(failures, "ExtractMemoriesConfig 缺 team_memory_enabled")
    if "team_memory_enabled: false" not in src:
        fail(failures, "ExtractMemoriesConfig 默认必须关闭 team memory")
    if "config.team_memory_enabled" not in src:
        fail(failures, "run_extraction 必须检查 config.team_memory_enabled")
    if "prompts::build_extract_combined_prompt" not in src:
        fail(failures, "team memory 启用时必须使用 combined extraction prompt")
    if "prompts::build_extract_auto_only_prompt" not in src:
        fail(failures, "auto-only extraction prompt 必须保留")
    if "extraction_prompt_uses_combined_memory_when_team_memory_enabled" not in src:
        fail(failures, "缺 combined extraction prompt 单测")


def check_herring_clock_not_in_team_code(failures: list[str]) -> None:
    for path in [MEMDIR, INTERACTIVE, EXTRACT]:
        code = strip_line_comments(read(path))
        if re.search(r"mossen_herring_clock|MOSSEN_HERRING_CLOCK", code):
            fail(
                failures,
                f"{path.relative_to(ROOT)} 的 team-memory 代码行不得包含 herring clock gate",
            )


def check_run_all_registration(failures: list[str]) -> None:
    if "wave_w50_team_memory_gate_smoke" not in read(RUN_ALL):
        fail(failures, "run_all_smoke.sh 未接入 W50 smoke")


def main() -> int:
    failures: list[str] = []
    check_memdir_gate(failures)
    check_config_alias_and_defaults(failures)
    check_runtime_snapshot(failures)
    check_extraction_prompt_gate(failures)
    check_herring_clock_not_in_team_code(failures)
    check_run_all_registration(failures)

    print("=== W50 team memory runtime gate smoke ===")
    print(f"memdir:      {MEMDIR.relative_to(ROOT)}")
    print(f"runtime:     {INTERACTIVE.relative_to(ROOT)}")
    print(f"alias map:   {ALIAS_MAP.relative_to(ROOT)}")
    print(f"defaults:    {DEFAULTS.relative_to(ROOT)}")
    print(f"extraction:  {EXTRACT.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: team-memory gate is independent from KAIROS and reaches runtime/extraction paths"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
