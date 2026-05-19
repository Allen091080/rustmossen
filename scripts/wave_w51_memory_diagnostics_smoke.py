#!/usr/bin/env python3
"""
W51 — memory runtime diagnostics smoke.

Verifies the extended /memory slash command and runtime_doctor_summary
include comprehensive, read-only memory diagnostics without leaking
secrets or file content.

/memory slash command (W51 extension):
  - Returns memory.runtime section with:
    autoMemoryEnabled, extractModeActive, teamMemory.{buildEnabled,enabled,
    rolloutEnabled,path}, sessionMemory.{enabled,compactEnabled,initialized},
    compact.{autoCompactEnabled}.
  - Does NOT return memory file content.
  - Does NOT return secrets or settings values.

runtime_doctor_summary (W51 extension):
  - memory check reports auto/extract/team/session status.
  - compact check reports auto-compact and slash bridge status.
  - No network/auth/spawn calls.

STOP conditions:
  - Does NOT write memory files.
  - Does NOT write configuration.
  - Does NOT modify query loop / compactConversation.
  - Does NOT add new compact execution paths.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PRINT_TS = ROOT / "cli" / "print.ts"
MEMDIR_PATHS = ROOT / "memdir" / "paths.ts"
TEAM_MEM_PATHS = ROOT / "memdir" / "teamMemPaths.ts"
CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def section(body: str, start: str, end: str) -> str:
    start_idx = body.find(start)
    if start_idx < 0:
        return ""
    end_idx = body.find(end, start_idx + len(start))
    if end_idx < 0:
        return body[start_idx:]
    return body[start_idx:end_idx]


def check_memory_runtime_section(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "command === 'memory'",
        "command === 'compact'",
    )
    if not block:
        fail(failures, "缺 /memory slash_command 分支")
        return
    # W51 runtime section must exist
    required_runtime = (
        "runtime:",
        "autoMemoryEnabled",
        "extractModeActive",
        "teamMemory:",
        "buildEnabled:",
        "enabled: teamMemEnabled",
        "rolloutEnabled:",
        "sessionMemory:",
        "compactEnabled:",
        "initialized:",
        "compact:",
        "autoCompactEnabled",
    )
    for token in required_runtime:
        if token not in block:
            fail(failures, f"/memory runtime section 缺锚点: {token}")


def check_memory_no_content(failures: list[str]) -> None:
    """Verify /memory does NOT echo file content (only length is ok)."""
    src = PRINT_TS.read_text()
    block = section(
        src,
        "command === 'memory'",
        "command === 'compact'",
    )
    if not block:
        return
    # file.content?.length is fine (computes size without returning content).
    # What we forbid is returning the content itself in the response payload.
    forbidden = (
        "content: file.content",
        "content: item.content",
        "readFile(",
        "readFileSync(",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"/memory 不得输出文件内容: {token}")


def check_memory_no_secrets(failures: list[str]) -> None:
    """Verify /memory does NOT return settings values or secrets."""
    src = PRINT_TS.read_text()
    block = section(
        src,
        "command === 'memory'",
        "command === 'compact'",
    )
    if not block:
        return
    forbidden = (
        "apiKey",
        "api_key",
        "secret",
        "token",
        "password",
        "credential",
    )
    code_lines = [
        line for line in block.splitlines()
        if not line.strip().startswith("//") and not line.strip().startswith("*")
    ]
    code_text = "\n".join(code_lines)
    for token in forbidden:
        if token in code_text:
            fail(failures, f"/memory 不得包含敏感字段: {token}")


def check_doctor_memory_check(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'runtime_doctor_summary'",
        "subtype === 'git_diff_summary'",
    )
    if not block:
        fail(failures, "缺 runtime_doctor_summary 分支")
        return
    required = (
        "id: 'memory'",
        "isAutoMemoryEnabled()",
        "isExtractModeActive()",
        "isTeamMemoryEnabled()",
        "isSessionMemoryInitialized()",
        "auto:",
        "extract:",
        "team:",
    )
    for token in required:
        if token not in block:
            fail(failures, f"doctor memory check 缺锚点: {token}")


def check_doctor_compact_check(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'runtime_doctor_summary'",
        "subtype === 'git_diff_summary'",
    )
    if not block:
        return
    required = (
        "id: 'compact'",
        "isAutoCompactEnabled()",
        "slash_bridge",
    )
    for token in required:
        if token not in block:
            fail(failures, f"doctor compact check 缺锚点: {token}")


def check_doctor_no_network(failures: list[str]) -> None:
    """Ensure doctor checks do NOT make network/spawn calls."""
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'runtime_doctor_summary'",
        "subtype === 'git_diff_summary'",
    )
    if not block:
        return
    forbidden = (
        "fetch(",
        "https://",
        "http://",
        "spawn(",
        "execSync(",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"doctor 不得做 network/spawn: {token}")


def check_auto_memory_still_enabled(failures: list[str]) -> None:
    """Verify isAutoMemoryEnabled is still exported from paths.ts."""
    src = MEMDIR_PATHS.read_text()
    if "export function isAutoMemoryEnabled()" not in src:
        fail(failures, "paths.ts 必须保留 isAutoMemoryEnabled 导出")


def check_team_memory_default_disabled(failures: list[str]) -> None:
    """Verify isTeamMemoryEnabled uses tengu_team_memory (not herring_clock)."""
    src = TEAM_MEM_PATHS.read_text()
    if "tengu_team_memory" not in src:
        fail(failures, "teamMemPaths.ts 必须使用 tengu_team_memory")
    func_start = src.find("export function isTeamMemoryEnabled()")
    if func_start < 0:
        fail(failures, "teamMemPaths.ts 缺 isTeamMemoryEnabled")
        return
    # Find function body end
    depth = 0
    func_body_start = src.find("{", func_start)
    func_end = func_body_start
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
            "isTeamMemoryEnabled 不得使用 tengu_herring_clock",
        )


def check_run_all_registration(failures: list[str]) -> None:
    src = RUN_ALL.read_text()
    if "wave_w51_memory_diagnostics_smoke" not in src:
        fail(failures, "run_all_smoke.sh 未接入 W51 smoke")


def main() -> int:
    failures: list[str] = []
    check_memory_runtime_section(failures)
    check_memory_no_content(failures)
    check_memory_no_secrets(failures)
    check_doctor_memory_check(failures)
    check_doctor_compact_check(failures)
    check_doctor_no_network(failures)
    check_auto_memory_still_enabled(failures)
    check_team_memory_default_disabled(failures)
    check_run_all_registration(failures)

    print("=== W51 memory diagnostics smoke ===")
    print(f"print.ts:       {PRINT_TS.relative_to(ROOT)}")
    print(f"paths.ts:       {MEMDIR_PATHS.relative_to(ROOT)}")
    print(f"teamMemPaths:   {TEAM_MEM_PATHS.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W51 memory diagnostics ✓ "
        "(/memory has runtime section, no content/secrets, "
        "doctor has memory+compact checks, no network/spawn, "
        "auto memory enabled, team memory uses tengu_team_memory)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
