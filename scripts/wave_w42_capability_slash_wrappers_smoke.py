#!/usr/bin/env python3
"""
W42/W46 — stream-json slash wrappers for existing CLI capabilities.

This smoke is intentionally static and contract-focused. It verifies that
skills, MCP, plugins, and agents are exposed as read-only slash_command wrappers
without calling TUI command bodies, installing plugins, writing config, spawning
agents, or leaking local paths. Runtime behavior for the underlying capability
systems is covered by wave_w41_cli_capability_matrix_smoke.py.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PRINT_TS = ROOT / "cli" / "print.ts"
CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
CAPABILITIES_TS = ROOT / "src" / "services" / "slashCommandCapabilities.ts"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"


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


def check_supported_set(body: str, failures: list[str]) -> None:
    caps = CAPABILITIES_TS.read_text()
    for command in ("skills", "mcp", "plugin", "agents"):
        if f"command: '{command}'" not in caps:
            fail(failures, f"capability manifest 缺 {command}")
    if "aliases: ['plugins']" not in caps:
        fail(failures, "plugin alias 'plugins' 未纳入 capability manifest")
    if "getStreamJsonSlashCommandCapabilities()" not in body:
        fail(failures, "/help 未回传 streamJsonCapabilities")
    if "isStreamJsonSlashCommandAvailable(c.name)" not in body:
        fail(failures, "/help supported 字段未从 capability manifest 计算")
    if "not wired through stream-json slash_command" not in body:
        fail(failures, "/help unsupported reason 未更新为通用 stream-json reason")


def check_skills(body: str, failures: list[str]) -> None:
    block = section(body, "command === 'skills'", "command === 'mcp'")
    if not block:
        fail(failures, "缺 command === 'skills' 分支")
        return
    required = (
        "getSlashCommandToolSkills(cwd())",
        "command: 'skills'",
        "skills: items",
        "unsupported_slash_command_args: skills",
        "hasWhenToUse",
    )
    for token in required:
        if token not in block:
            fail(failures, f"skills 分支缺契约锚点: {token}")
    forbidden = ("skillRoot", "content:", "paths:", "path:")
    for token in forbidden:
        if token in block:
            fail(failures, f"skills 分支不应回传本地路径或技能正文: {token}")


def check_mcp(body: str, failures: list[str]) -> None:
    block = section(body, "command === 'mcp'", "command === 'plugin'")
    if not block:
        fail(failures, "缺 command === 'mcp' 分支")
        return
    required = (
        "buildMcpServerStatuses()",
        "command: 'mcp'",
        "mcp:",
        "toolCount",
        "unsupported_slash_command_args: mcp",
    )
    for token in required:
        if token not in block:
            fail(failures, f"mcp 分支缺契约锚点: {token}")
    # Reject raw MCP config shape; comments may mention the words.
    for token in ("config:", "headers:"):
        if token in block:
            fail(failures, f"mcp 分支不应回传 raw server config: {token}")


def check_plugin(body: str, failures: list[str]) -> None:
    block = section(body, "command === 'plugin'", "command === 'agents'")
    if not block:
        fail(failures, "缺 command === 'plugin' 分支")
        return
    required = (
        "command === 'plugin'",
        "loadAllPluginsCacheOnly()",
        "command: 'plugin'",
        "plugins:",
        "enabled,",
        "disabled,",
        "errorCount",
        "sourceKind",
        "unsupported_slash_command_args",
    )
    for token in required:
        if token not in block:
            fail(failures, f"plugin 分支缺契约锚点: {token}")
    forbidden = (
        "path:",
        "plugin.path",
        "repository:",
        "sha:",
        "installPlugin",
        "refreshActivePlugins",
        "updateSettings",
        "saveGlobalConfig",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"plugin 分支不应泄漏路径或产生副作用: {token}")


def check_agents(body: str, failures: list[str]) -> None:
    block = section(body, "command === 'agents'", "command === 'permissions'")
    if not block:
        fail(failures, "缺 command === 'agents' 分支")
        return
    required = (
        "command === 'agents'",
        "currentAgents.map",
        "command: 'agents'",
        "agents,",
        "hasTools",
        "hasSkills",
        "hasMcpServers",
        "unsupported_slash_command_args: agents",
    )
    for token in required:
        if token not in block:
            fail(failures, f"agents 分支缺契约锚点: {token}")
    forbidden = (
        "getSystemPrompt",
        "baseDir",
        "filename",
        "initialPrompt",
        "criticalSystemReminder",
        "prompt:",
        "path:",
        "spawn",
        "runAgent",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"agents 分支不应回传 prompt/path 或启动 agent: {token}")


def check_compact_still_blocked(body: str, failures: list[str]) -> None:
    block = section(body, "command === 'compact'", "command === ''")
    if "sendControlResponseSuccess" in block:
        fail(failures, "/compact 仍必须 blocked, 不得走 success")
    if "compactConversation" in block:
        fail(failures, "/compact 分支不得调用 compactConversation")


def check_schema_description(failures: list[str]) -> None:
    src = CONTROL_SCHEMAS.read_text()
    for token in ("`skills`", "`mcp`", "`plugin`", "`agents`"):
        if token not in src:
            fail(failures, f"control schema describe 缺 {token}")


def check_run_all_registration(failures: list[str]) -> None:
    src = RUN_ALL.read_text()
    if "wave_w42_capability_slash_wrappers_smoke" not in src:
        fail(failures, "run_all_smoke.sh 未接入 W42 capability slash wrapper smoke")


def main() -> int:
    failures: list[str] = []
    try:
        body = slash_branch()
    except RuntimeError as exc:
        failures.append(str(exc))
        body = ""

    if body:
        check_supported_set(body, failures)
        check_skills(body, failures)
        check_mcp(body, failures)
        check_plugin(body, failures)
        check_agents(body, failures)
        check_compact_still_blocked(body, failures)
    check_schema_description(failures)
    check_run_all_registration(failures)

    print("=== W42 capability slash wrappers smoke ===")
    print(f"print.ts:          {PRINT_TS.relative_to(ROOT)}")
    print(f"controlSchemas.ts: {CONTROL_SCHEMAS.relative_to(ROOT)}")
    print(f"capabilities.ts:   {CAPABILITIES_TS.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print("PASS: skills/mcp/plugin/agents read-only slash wrappers ✓")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
