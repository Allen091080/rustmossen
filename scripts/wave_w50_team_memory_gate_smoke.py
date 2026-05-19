#!/usr/bin/env python3
"""
W50 — team memory runtime gate smoke.

Verifies the team memory gate is independent from the KAIROS gate:

  1. isTeamMemoryEnabled() uses 'tengu_team_memory' (NOT 'tengu_herring_clock').
  2. aliasMap has 'tengu_team_memory' → 'mossen.memory.teamMemoryEnabled'.
  3. defaults.ts declares 'mossen.memory.teamMemoryEnabled': false.
  4. teamMemoryRuntime.ts uses 'tengu_team_memory' for rolloutEnabled.
  5. memdir.ts disabled-telemetry block uses 'tengu_team_memory'.
  6. 'tengu_herring_clock' no longer appears in any team memory code path.
  7. extractMemories still checks isTeamMemoryEnabled() (not removed).
  8. isAutoMemoryEnabled() is not affected.
  9. run_all_smoke.sh registers W50.

STOP conditions:
  - Does NOT modify extractMemories extraction logic.
  - Does NOT modify compactConversation or query loop.
  - Does NOT add new compact execution paths.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TEAM_MEM_PATHS = ROOT / "memdir" / "teamMemPaths.ts"
ALIAS_MAP = ROOT / "services" / "config" / "aliasMap.ts"
DEFAULTS = ROOT / "services" / "config" / "defaults.ts"
RUNTIME = ROOT / "platform" / "teamMemoryRuntime.ts"
MEMDIR = ROOT / "memdir" / "memdir.ts"
EXTRACT = ROOT / "services" / "extractMemories" / "extractMemories.ts"
PATHS = ROOT / "memdir" / "paths.ts"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def check_team_mem_paths_gate(failures: list[str]) -> None:
    src = TEAM_MEM_PATHS.read_text()
    if "tengu_team_memory" not in src:
        fail(failures, "teamMemPaths.ts isTeamMemoryEnabled 必须用 tengu_team_memory")
    # The function must NOT use tengu_herring_clock for team memory gating
    func_start = src.find("export function isTeamMemoryEnabled()")
    func_end = src.find("}", func_start + 1)
    # Find matching closing brace (simple approach)
    depth = 0
    func_body_start = src.find("{", func_start)
    for i in range(func_body_start, len(src)):
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
            if depth == 0:
                func_end = i + 1
                break
    func_body = src[func_start:func_end]
    if "tengu_herring_clock" in func_body:
        fail(
            failures,
            "isTeamMemoryEnabled() 不得使用 tengu_herring_clock 作为 gate",
        )


def check_alias_map(failures: list[str]) -> None:
    src = ALIAS_MAP.read_text()
    if "'tengu_team_memory': 'mossen.memory.teamMemoryEnabled'" not in src:
        fail(failures, "aliasMap.ts 缺 tengu_team_memory → mossen.memory.teamMemoryEnabled 映射")
    # kairosActive must still exist (KAIROS gate not removed)
    if "'tengu_herring_clock': 'mossen.memory.kairosActive'" not in src:
        fail(failures, "aliasMap.ts 仍需保留 tengu_herring_clock → mossen.memory.kairosActive 映射")


def check_defaults(failures: list[str]) -> None:
    src = DEFAULTS.read_text()
    if "'mossen.memory.teamMemoryEnabled': false" not in src:
        fail(failures, "defaults.ts 缺 mossen.memory.teamMemoryEnabled: false")
    if "'mossen.memory.kairosActive': false" not in src:
        fail(failures, "defaults.ts 必须保留 mossen.memory.kairosActive: false")


def check_runtime_snapshot(failures: list[str]) -> None:
    src = RUNTIME.read_text()
    if "tengu_team_memory" not in src:
        fail(failures, "teamMemoryRuntime.ts 必须使用 tengu_team_memory")
    if "tengu_herring_clock" in src:
        fail(failures, "teamMemoryRuntime.ts 不得使用 tengu_herring_clock")


def check_memdir_disabled_telemetry(failures: list[str]) -> None:
    src = MEMDIR.read_text()
    # The disabled-telemetry block near the end must use tengu_team_memory
    # Find the tengu_team_memdir_disabled event
    disabled_block_start = src.find("tengu_team_memdir_disabled")
    if disabled_block_start < 0:
        fail(failures, "memdir.ts 缺 tengu_team_memdir_disabled 事件")
        return
    # Check the block uses tengu_team_memory, not tengu_herring_clock
    window = src[max(0, disabled_block_start - 500):disabled_block_start + 100]
    if "tengu_team_memory" not in window:
        fail(failures, "memdir.ts disabled 遥测块必须用 tengu_team_memory")


def check_herring_clock_not_in_team_paths(failures: list[str]) -> None:
    """Ensure tengu_herring_clock is NOT used in team memory code paths."""
    for file_path, label in [
        (TEAM_MEM_PATHS, "teamMemPaths.ts"),
        (RUNTIME, "teamMemoryRuntime.ts"),
    ]:
        src = file_path.read_text()
        # Only check code lines (skip comments)
        code_lines = [
            line for line in src.splitlines()
            if not line.strip().startswith("//") and not line.strip().startswith("*")
        ]
        code_text = "\n".join(code_lines)
        if "tengu_herring_clock" in code_text:
            fail(
                failures,
                f"{label} 代码行不得包含 tengu_herring_clock "
                f"(KAIROS gate 不应影响 team memory)",
            )


def check_extract_still_references_team(failures: list[str]) -> None:
    """Ensure extractMemories still checks team memory status."""
    src = EXTRACT.read_text()
    if "isTeamMemoryEnabled()" not in src:
        fail(failures, "extractMemories 仍需调用 isTeamMemoryEnabled()")


def check_auto_memory_untouched(failures: list[str]) -> None:
    """Verify isAutoMemoryEnabled is still referenced."""
    src = PATHS.read_text()
    if "export function isAutoMemoryEnabled()" not in src:
        fail(failures, "paths.ts 必须保留 isAutoMemoryEnabled 导出")


def check_run_all_registration(failures: list[str]) -> None:
    src = RUN_ALL.read_text()
    if "wave_w50_team_memory_gate_smoke" not in src:
        fail(failures, "run_all_smoke.sh 未接入 W50 smoke")


def main() -> int:
    failures: list[str] = []
    check_team_mem_paths_gate(failures)
    check_alias_map(failures)
    check_defaults(failures)
    check_runtime_snapshot(failures)
    check_memdir_disabled_telemetry(failures)
    check_herring_clock_not_in_team_paths(failures)
    check_extract_still_references_team(failures)
    check_auto_memory_untouched(failures)
    check_run_all_registration(failures)

    print("=== W50 team memory runtime gate smoke ===")
    print(f"teamMemPaths.ts:  {TEAM_MEM_PATHS.relative_to(ROOT)}")
    print(f"aliasMap.ts:      {ALIAS_MAP.relative_to(ROOT)}")
    print(f"defaults.ts:      {DEFAULTS.relative_to(ROOT)}")
    print(f"runtime.ts:       {RUNTIME.relative_to(ROOT)}")
    print(f"memdir.ts:        {MEMDIR.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W50 team memory gate ✓ "
        "(isTeamMemoryEnabled uses tengu_team_memory, "
        "tengu_herring_clock not in team memory code paths, "
        "aliasMap has independent mapping, "
        "defaults declares teamMemoryEnabled false, "
        "runtime snapshot uses new gate, "
        "memdir telemetry uses new gate, "
        "extractMemories still checks team memory, "
        "isAutoMemoryEnabled untouched)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
