#!/usr/bin/env python3
"""
W43 — stream-json slash capability manifest contract.

The CLI/Core repo is the source of truth for Workbench-facing slash capability
availability. This smoke keeps the manifest machine-readable, explicit, and
aligned with cli/print.ts dispatch.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CAPABILITIES_TS = ROOT / "src" / "services" / "slashCommandCapabilities.ts"
PRINT_TS = ROOT / "cli" / "print.ts"
PROTOCOL_CONTRACT = ROOT / "docs" / "reference" / "protocol-contract.md"
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
)
BLOCKED = ("compact",)


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def check_manifest_shape(src: str, failures: list[str]) -> None:
    required_exports = (
        "STREAM_JSON_SLASH_COMMAND_CAPABILITIES",
        "getStreamJsonSlashCommandCapabilities",
        "serializeStreamJsonSlashCommandCapability",
        "getStreamJsonSlashCommandCapabilityManifest",
        "normalizeStreamJsonSlashCommand",
        "getStreamJsonSlashCommandCapability",
        "isStreamJsonSlashCommandAvailable",
        "formatAvailableStreamJsonSlashCommands",
        "STREAM_JSON_SLASH_CAPABILITY_MANIFEST_VERSION",
    )
    for token in required_exports:
        if token not in src:
            fail(failures, f"manifest 缺导出: {token}")

    for command in AVAILABLE:
        if f"command: '{command}'" not in src:
            fail(failures, f"manifest 缺 available command: {command}")
    for command in BLOCKED:
        if f"command: '{command}'" not in src:
            fail(failures, f"manifest 缺 blocked command: {command}")
    if "aliases: ['plugins']" not in src:
        fail(failures, "manifest 缺 plugin alias: plugins")

    if src.count("status: 'available'") < len(AVAILABLE):
        fail(failures, "available status 数量不足")
    if "status: 'blocked'" not in src:
        fail(failures, "blocked status 缺失")
    if "requiresConfirmation: true" not in src:
        fail(failures, "clear 需要 requiresConfirmation: true")
    if "sideEffect: 'clears_conversation'" not in src:
        fail(failures, "clear 需要 sideEffect clears_conversation")
    if "argsMode: 'profile_name'" not in src:
        fail(failures, "model 需要 argsMode profile_name")
    if "sideEffect: 'switches_session_model'" not in src:
        fail(failures, "model 需要 sideEffect switches_session_model")
    if "acceptedArgs: ['--confirm']" not in src:
        fail(failures, "clear 需要 acceptedArgs: ['--confirm']")
    if src.count("acceptedArgs: []") < len(AVAILABLE) - 1 + len(BLOCKED):
        fail(failures, "无参命令需要显式 acceptedArgs: []")
    for result_kind in AVAILABLE + ("error",):
        if f"resultKind: '{result_kind}'" not in src:
            fail(failures, f"manifest 缺 resultKind: {result_kind}")
    for payload_key in (
        "commands",
        "streamJsonCapabilities",
        "capabilities",
        "runtime",
        "model",
        "clear",
        "cost",
        "skills",
        "mcp",
        "plugins",
        "agents",
    ):
        if payload_key not in src:
            fail(failures, f"manifest 缺 payloadKeys: {payload_key}")


def slash_branch() -> str:
    src = PRINT_TS.read_text()
    match = re.search(
        r"message\.request\.subtype === 'slash_command'\)\s*\{([\s\S]*)\}\s*else\s*\{[\s\S]*?Unknown control request subtype",
        src,
    )
    return match.group(1) if match else ""


def check_print_integration(failures: list[str]) -> None:
    body = slash_branch()
    if not body:
        fail(failures, "无法抓取 print.ts slash_command 分支")
        return
    required = (
        "normalizeStreamJsonSlashCommand(rawCommand)",
        "getStreamJsonSlashCommandCapabilityManifest()",
        "getStreamJsonSlashCommandCapabilities()",
        "isStreamJsonSlashCommandAvailable(c.name)",
        "formatAvailableStreamJsonSlashCommands()",
        "streamJsonCapabilities:",
        "command === 'capabilities'",
        "manifestVersion: STREAM_JSON_SLASH_CAPABILITY_MANIFEST_VERSION",
    )
    for token in required:
        if token not in body:
            fail(failures, f"print.ts 未消费 manifest: {token}")
    for command in AVAILABLE:
        if f"command === '{command}'" not in body:
            fail(failures, f"print.ts 缺 dispatch 分支: {command}")
    if "command === 'compact'" not in body:
        fail(failures, "print.ts 缺 compact blocked 分支")


def check_run_all_registration(failures: list[str]) -> None:
    src = RUN_ALL.read_text()
    if "wave_w43_slash_capability_manifest_smoke" not in src:
        fail(failures, "run_all_smoke.sh 未接入 W43 manifest smoke")


def check_protocol_contract(failures: list[str]) -> None:
    src = PROTOCOL_CONTRACT.read_text()
    required_tokens = (
        "### 2.6 `slash_command`",
        "services/slashCommandCapabilities.ts",
        "Workbench 启动后应先请求 `/capabilities`",
        "`compact` | blocked",
        "`resultKind`",
        "`payloadKeys`",
        "`acceptedArgs`",
        "`manifestVersion`",
        "不得在 control_request handler 内伪实现",
        "model{current,source,available[],profiles[],switched?}",
        "plugins{enabled[],disabled[],errorCount}",
        "agents[]",
        "cost{totalCostUsd,durations,tokens,lines}",
    )
    for token in required_tokens:
        if token not in src:
            fail(failures, f"protocol-contract.md 缺 slash capability 契约锚点: {token}")
    for command in AVAILABLE + BLOCKED:
        if f"`{command}`" not in src:
            fail(failures, f"protocol-contract.md 缺 command: {command}")


def main() -> int:
    failures: list[str] = []
    src = CAPABILITIES_TS.read_text()
    check_manifest_shape(src, failures)
    check_print_integration(failures)
    check_run_all_registration(failures)
    check_protocol_contract(failures)

    print("=== W43 slash capability manifest smoke ===")
    print(f"capabilities.ts: {CAPABILITIES_TS.relative_to(ROOT)}")
    print(f"print.ts:        {PRINT_TS.relative_to(ROOT)}")
    print(f"contract:        {PROTOCOL_CONTRACT.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print("PASS: slash capability manifest is source of truth ✓")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
