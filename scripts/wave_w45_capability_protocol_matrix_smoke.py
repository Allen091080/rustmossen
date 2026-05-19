#!/usr/bin/env python3
"""
W45 — stream-json slash capability matrix end-to-end contract smoke.

Locks the full capability matrix exposed via stream-json `slash_command`:

  1. Every Allen-listed command exists in the manifest with status
     'available' or 'blocked'.
  2. Every available command has a real dispatcher branch.
  3. Every blocked command either has its own branch (compact) or is
     handled by the generic isStreamJsonSlashCommandBlocked() fall-through.
  4. Manifest entries carry the W45 metadata fields (id, title, kind,
     protocol, errorTags, source, lastVerifiedBySmoke).
  5. New read-only wrappers (/permissions, /hooks, /memory) reject extra
     args and never echo secrets/content/paths.
  6. /compact is never wired to compactConversation from this dispatcher.
  7. New blocked commands fall through to a `blocked_slash_command:` tag.
  8. run_all_smoke.sh registers W45.
  9. manifestVersion has been bumped past 1 (matrix expansion).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PRINT_TS = ROOT / "cli" / "print.ts"
CAPABILITIES_TS = ROOT / "src" / "services" / "slashCommandCapabilities.ts"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"

AVAILABLE = (
    "help",
    "capabilities",
    "status",
    "model",
    "clear",
    "cost",
    "skills",
    "mcp",
    "plugin",
    "agents",
    "permissions",
    "hooks",
    "memory",
)

BLOCKED = (
    "compact",
    "context",
    "config",
    "profile",
    "doctor",
    "diff",
    "ide",
    "init",
    "login",
    "logout",
)

# Aliases lock: count = sum of aliases[] across canonical entries.
# canonical_command: tuple_of_alias_strings — alphabetised.
EXPECTED_ALIASES = {
    "capabilities": ("capability",),
    "plugin": ("plugins",),
    "permissions": ("allowed-tools",),
    "config": ("settings",),
}


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def slash_branch() -> str:
    src = PRINT_TS.read_text()
    match = re.search(
        r"message\.request\.subtype === 'slash_command'\)\s*\{([\s\S]*)\}\s*else\s*\{[\s\S]*?Unknown control request subtype",
        src,
    )
    if not match:
        raise RuntimeError("slash_command branch not found")
    return match.group(1)


def section(body: str, start: str, end: str) -> str:
    start_idx = body.find(start)
    if start_idx < 0:
        return ""
    end_idx = body.find(end, start_idx + len(start))
    if end_idx < 0:
        return body[start_idx:]
    return body[start_idx:end_idx]


def check_manifest_entries(failures: list[str]) -> None:
    src = CAPABILITIES_TS.read_text()
    if "STREAM_JSON_SLASH_CAPABILITY_MANIFEST_VERSION = 2" not in src:
        fail(
            failures,
            "manifestVersion 必须 ≥ 2（capability matrix 已扩展）",
        )
    for command in AVAILABLE:
        if f"command: '{command}'" not in src:
            fail(failures, f"manifest 缺 available command: {command}")
        if f"id: 'slash.{command}'" not in src:
            fail(failures, f"manifest 缺 id: slash.{command}")
    for command in BLOCKED:
        if f"command: '{command}'" not in src:
            fail(failures, f"manifest 缺 blocked command: {command}")
        if f"id: 'slash.{command}'" not in src:
            fail(failures, f"manifest 缺 id: slash.{command}")
    required_fields = (
        "title:",
        "kind: 'slash_command'",
        "protocol: 'stream_json'",
        "errorTags:",
        "source:",
        "lastVerifiedBySmoke:",
    )
    for token in required_fields:
        if token not in src:
            fail(failures, f"manifest 缺新字段: {token}")
    # Counting rule: canonical command count only. Aliases live in
    # `aliases: [...]` on the owning entry and are NOT separate manifest
    # entries. Total canonical = 13 available + 10 blocked = 23.
    canonical_count = src.count("command: '")
    expected_canonical = len(AVAILABLE) + len(BLOCKED)
    if canonical_count != expected_canonical:
        fail(
            failures,
            f"canonical entry count {canonical_count} != expected {expected_canonical} "
            f"(13 available + 10 blocked)",
        )
    available_count = src.count("status: 'available'")
    blocked_count = src.count("status: 'blocked'")
    if available_count != len(AVAILABLE):
        fail(
            failures,
            f"available 状态数 = {available_count}, expected {len(AVAILABLE)}",
        )
    if blocked_count != len(BLOCKED):
        fail(
            failures,
            f"blocked 状态数 = {blocked_count}, expected {len(BLOCKED)}",
        )
    # Alias lock: each canonical command's aliases[] must match exactly.
    for canonical, expected_aliases in EXPECTED_ALIASES.items():
        for alias in expected_aliases:
            anchor = f"command: '{canonical}'"
            start = src.find(anchor)
            if start < 0:
                fail(failures, f"alias check 找不到 canonical anchor: {canonical}")
                continue
            # search the next ~30 lines for the aliases line
            window = src[start : start + 1500]
            if f"aliases: ['{alias}']" not in window and f"'{alias}'" not in window:
                fail(
                    failures,
                    f"canonical {canonical} 的 alias `{alias}` 未声明",
                )


def check_dispatcher_available(body: str, failures: list[str]) -> None:
    for command in AVAILABLE:
        token = f"command === '{command}'"
        if token not in body:
            fail(failures, f"dispatcher 缺 available 分支: {token}")


def check_dispatcher_blocked(body: str, failures: list[str]) -> None:
    if "command === 'compact'" not in body:
        fail(failures, "dispatcher 缺 compact 显式分支")
    if "isStreamJsonSlashCommandBlocked(command)" not in body:
        fail(
            failures,
            "dispatcher 缺通用 isStreamJsonSlashCommandBlocked fall-through",
        )
    if "blocked_slash_command: ${command}" not in body:
        fail(
            failures,
            "blocked fall-through 必须用 `blocked_slash_command: ${command}` tag",
        )


def check_permissions_branch(body: str, failures: list[str]) -> None:
    block = section(body, "command === 'permissions'", "command === 'hooks'")
    if not block:
        fail(failures, "缺 command === 'permissions' 分支")
        return
    required = (
        "command: 'permissions'",
        "subtype: 'slash_command_result'",
        "status: 'completed'",
        "permissions: {",
        "getAppState().toolPermissionContext",
        "alwaysAllowRuleCounts",
        "alwaysDenyRuleCounts",
        "alwaysAskRuleCounts",
        "isBypassPermissionsModeAvailable",
        "additionalWorkingDirectoryCount",
        "unsupported_slash_command_args: permissions",
    )
    for token in required:
        if token not in block:
            fail(failures, f"permissions 分支缺锚点: {token}")
    forbidden = (
        "alwaysAllowRules:",
        "alwaysDenyRules:",
        "alwaysAskRules:",
        "additionalWorkingDirectories:",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"permissions 分支不应回传原始 rule patterns: {token}")


def check_hooks_branch(body: str, failures: list[str]) -> None:
    block = section(body, "command === 'hooks'", "command === 'memory'")
    if not block:
        fail(failures, "缺 command === 'hooks' 分支")
        return
    required = (
        "command: 'hooks'",
        "getAllHooks(getAppState())",
        "byEvent",
        "bySource",
        "byType",
        "unsupported_slash_command_args: hooks",
    )
    for token in required:
        if token not in block:
            fail(failures, f"hooks 分支缺锚点: {token}")
    forbidden = (
        "h.config.command",
        "h.config.url",
        "h.config.prompt",
        "config: hookCommand",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"hooks 分支不应回传命令体/URL/prompt: {token}")


def check_memory_branch(body: str, failures: list[str]) -> None:
    block = section(body, "command === 'memory'", "command === 'compact'")
    if not block:
        fail(failures, "缺 command === 'memory' 分支")
        return
    required = (
        "command: 'memory'",
        "await getMemoryFiles()",
        "memory: {",
        "files:",
        "contentLength",
        "unsupported_slash_command_args: memory",
    )
    for token in required:
        if token not in block:
            fail(failures, f"memory 分支缺锚点: {token}")
    forbidden = (
        "content: file.content",
        "rawContent: file.rawContent",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"memory 分支不应回传文件内容: {token}")


def check_compact_blocked(body: str, failures: list[str]) -> None:
    block = section(body, "command === 'compact'", "isStreamJsonSlashCommandBlocked")
    if "compactConversation" in block:
        fail(failures, "/compact 分支不得调用 compactConversation")
    if "sendControlResponseSuccess" in block:
        fail(failures, "/compact 分支不得走 success 路径")


def check_run_all_registration(failures: list[str]) -> None:
    src = RUN_ALL.read_text()
    if "wave_w45_capability_protocol_matrix_smoke" not in src:
        fail(failures, "run_all_smoke.sh 未接入 W45 capability matrix smoke")


def main() -> int:
    failures: list[str] = []
    try:
        body = slash_branch()
    except RuntimeError as exc:
        failures.append(str(exc))
        body = ""

    check_manifest_entries(failures)
    if body:
        check_dispatcher_available(body, failures)
        check_dispatcher_blocked(body, failures)
        check_permissions_branch(body, failures)
        check_hooks_branch(body, failures)
        check_memory_branch(body, failures)
        check_compact_blocked(body, failures)
    check_run_all_registration(failures)

    alias_count = sum(len(v) for v in EXPECTED_ALIASES.values())
    print("=== W45 capability protocol matrix smoke ===")
    print(f"capabilities.ts: {CAPABILITIES_TS.relative_to(ROOT)}")
    print(f"print.ts:        {PRINT_TS.relative_to(ROOT)}")
    print(f"canonical:       {len(AVAILABLE) + len(BLOCKED)} ({len(AVAILABLE)} available + {len(BLOCKED)} blocked)")
    print(f"aliases:         {alias_count}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        f"PASS: capability matrix ✓ ("
        f"{len(AVAILABLE) + len(BLOCKED)} canonical = "
        f"{len(AVAILABLE)} available + {len(BLOCKED)} blocked, "
        f"{alias_count} aliases, manifestVersion=2)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
