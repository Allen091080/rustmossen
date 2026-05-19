#!/usr/bin/env python3
"""
W47 — real capability operations contract smoke.

Locks the W47 protocol additions:

  Real implementations:
    - get_capability_operations: discovery-only routing map; never executes;
      lists each (capabilityId, operation) with its existing safe executor
      subtype and confirmation rules.
    - git_diff_summary.includePatch: bounded patch preview (100KB cap,
      20 file cap, binary detection via numstat); subprocess timeout
      identical to no-patch path; never echoes patch when off.

  Schema-locked blocked-with-reason:
    - apply_config_change: always status='blocked' with stable reason.
    - project_memory_operation: always status='blocked' with stable reason.

  Schema-only extensions (deferred implementation, but wire-compatible
  today):
    - runtime_doctor_summary.includeNetworkProbes: when true the response
      MUST include probe_disabled_in_this_build placeholders, never make
      a real network call.

  STOP / red lines locked:
    - compact_conversation still always blocked, never imports
      compactConversation, never returns status='completed'.
    - No auth/login/logout mutation path is wired.
    - Slash entries for /compact /config /doctor /diff /ide stay blocked.
    - W45 / W46 invariants do not regress.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PRINT_TS = ROOT / "cli" / "print.ts"
CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
WHITELIST = ROOT / "scripts" / "stream-json-schema-whitelist.txt"
CAPABILITIES_TS = ROOT / "src" / "services" / "slashCommandCapabilities.ts"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"

NEW_SUBTYPES = (
    "apply_config_change",
    "get_capability_operations",
    "project_memory_operation",
)

NEW_SCHEMA_NAMES = (
    "SDKControlApplyConfigChangeRequestSchema",
    "SDKControlApplyConfigChangeResponseSchema",
    "SDKControlGetCapabilityOperationsRequestSchema",
    "SDKControlGetCapabilityOperationsResponseSchema",
    "SDKControlProjectMemoryOperationRequestSchema",
    "SDKControlProjectMemoryOperationResponseSchema",
)


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


def check_schemas(failures: list[str]) -> None:
    src = CONTROL_SCHEMAS.read_text()
    for name in NEW_SCHEMA_NAMES:
        if f"export const {name}" not in src:
            fail(failures, f"controlSchemas.ts 缺导出: {name}")
    for subtype in NEW_SUBTYPES:
        if f"z.literal('{subtype}')" not in src:
            fail(failures, f"controlSchemas.ts 缺 z.literal('{subtype}')")
    # Union must have all 3 new request schemas.
    union_match = re.search(
        r"SDKControlRequestInnerSchema\s*=\s*lazySchema\(\(\)\s*=>\s*\n?\s*z\.union\(\[([\s\S]*?)\]\)",
        src,
    )
    if not union_match:
        fail(failures, "找不到 SDKControlRequestInnerSchema union 体")
        return
    union_body = union_match.group(1)
    for name in (
        "SDKControlApplyConfigChangeRequestSchema",
        "SDKControlGetCapabilityOperationsRequestSchema",
        "SDKControlProjectMemoryOperationRequestSchema",
    ):
        if name not in union_body:
            fail(failures, f"SDKControlRequestInner union 缺成员: {name}")
    # git_diff_summary request must declare includePatch.
    git_diff_block = section(
        src,
        "SDKControlGitDiffSummaryRequestSchema",
        "SDKControlGitDiffSummaryResponseSchema",
    )
    if "includePatch" not in git_diff_block:
        fail(failures, "git_diff_summary request schema 缺 includePatch")
    # git_diff_summary response must declare patch object with the listed keys.
    git_diff_resp = section(
        src,
        "SDKControlGitDiffSummaryResponseSchema",
        "// W47 — Real Capability Operations",
    )
    for key in (
        "patch:",
        "binaryFiles",
        "skippedFiles",
        "totalBytes",
        "fileCount",
    ):
        if key not in git_diff_resp:
            fail(failures, f"git_diff_summary response 缺 patch.{key}")
    # runtime_doctor_summary request must declare includeNetworkProbes.
    doctor_block = section(
        src,
        "SDKControlRuntimeDoctorSummaryRequestSchema",
        "SDKControlRuntimeDoctorSummaryResponseSchema",
    )
    if "includeNetworkProbes" not in doctor_block:
        fail(
            failures,
            "runtime_doctor_summary request 缺 includeNetworkProbes",
        )


def check_whitelist(failures: list[str]) -> None:
    src = WHITELIST.read_text()
    if "Section B — SDKControlRequestInner union (29 成员" not in src:
        fail(failures, "whitelist Section B header 未升 29 成员")
    for name in (
        "SDKControlApplyConfigChangeRequestSchema",
        "SDKControlGetCapabilityOperationsRequestSchema",
        "SDKControlProjectMemoryOperationRequestSchema",
    ):
        if name not in src:
            fail(failures, f"whitelist 缺 Section B 成员: {name}")


def check_dispatch_apply_config_change(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'apply_config_change'",
        "subtype === 'get_capability_operations'",
    )
    if not block:
        fail(failures, "缺 apply_config_change 分支")
        return
    if "status: 'applied'" in block:
        fail(failures, "apply_config_change 必须 blocked，不得返回 'applied'")
    if "status: 'blocked'" not in block:
        fail(failures, "apply_config_change 必须返回 status:'blocked'")
    if "reason:" not in block:
        fail(failures, "apply_config_change blocked 必须带 reason")
    forbidden = (
        "updateSettingsForSource(",
        "writeFileSyncAndFlush",
        "saveGlobalConfig",
        "fs.writeFile",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"apply_config_change 不得调用真实写入: {token}")


def check_dispatch_get_capability_operations(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'get_capability_operations'",
        "subtype === 'project_memory_operation'",
    )
    if not block:
        fail(failures, "缺 get_capability_operations 分支")
        return
    required = (
        "operations:",
        "capabilityId: 'plugins'",
        "capabilityId: 'mcp'",
        "capabilityId: 'skills'",
        "capabilityId: 'config'",
        "capabilityId: 'project_memory'",
        "capabilityId: 'compact'",
        "capabilityId: 'auth'",
        "executor: 'reload_plugins'",
        "executor: 'mcp_set_servers'",
        "executor: 'mcp_reconnect'",
        "executor: 'mcp_toggle'",
        "executor: 'capability_recommendation_response'",
        "executor: 'apply_config_change'",
        "executor: 'apply_flag_settings'",
        "executor: 'project_memory_operation'",
        "executor: 'compact_conversation'",
    )
    for token in required:
        if token not in block:
            fail(failures, f"get_capability_operations 缺 routing entry: {token}")
    forbidden = (
        "installPlugin(",
        "spawn(",
        "execSync(",
        "writeFileSyncAndFlush",
        "updateSettingsForSource(",
    )
    for token in forbidden:
        if token in block:
            fail(
                failures,
                f"get_capability_operations 必须仅做发现，不得执行: {token}",
            )


def check_dispatch_project_memory(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'project_memory_operation'",
        "subtype === 'slash_command'",
    )
    if not block:
        fail(failures, "缺 project_memory_operation 分支")
        return
    if "status: 'blocked'" not in block:
        fail(failures, "project_memory_operation 必须返回 status:'blocked'")
    if "reason:" not in block:
        fail(failures, "project_memory_operation blocked 必须带 reason")
    forbidden = (
        "writeFileSync",
        "writeFile(",
        "fs.write",
        "appendFile",
        "mkdirSync",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"project_memory_operation 不得真写文件: {token}")


def check_git_diff_patch_preview(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'git_diff_summary'",
        "subtype === 'apply_config_change'",
    )
    if not block:
        fail(failures, "缺 git_diff_summary 分支")
        return
    required = (
        "includePatch",
        "PATCH_BYTE_CAP = 100 * 1024",
        "PATCH_FILE_CAP = 20",
        "diff', '--numstat",
        "binaryFiles",
        "skippedFiles",
        "totalBytes",
        "patch: patchPayload",
    )
    for token in required:
        if token not in block:
            fail(failures, f"git_diff_summary patch 扩展缺锚点: {token}")
    forbidden = (
        "git apply",
        "git push",
        "git commit",
        "git checkout",
        "git reset",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"git_diff_summary patch 不得做 git mutation: {token}")


def check_doctor_probe_extension(failures: list[str]) -> None:
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
        "includeNetworkProbes",
        "probe_disabled_in_this_build",
        "id: 'network_probe'",
        "id: 'version_probe'",
    )
    for token in required:
        if token not in block:
            fail(failures, f"doctor probe 扩展缺锚点: {token}")
    forbidden = (
        "fetch(",
        "https://",
        "http://",
        "spawn(",
        "execSync(",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"doctor probe 不得做真实网络调用: {token}")


def check_compact_still_blocked(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'compact_conversation'",
        "subtype === 'get_config_summary'",
    )
    # Only check code lines for forbidden tokens (skip comments)
    code_lines = [
        line for line in block.splitlines()
        if not line.strip().startswith("//") and not line.strip().startswith("*")
    ]
    code_text = "\n".join(code_lines)
    if "compactConversation" in code_text:
        fail(failures, "compact_conversation 仍不得调用 compactConversation")
    if "status: 'completed'" in block:
        fail(failures, "compact_conversation 仍不得返回 'completed'")
    if "status: 'blocked'" not in block:
        fail(failures, "compact_conversation 仍必须 blocked")


def check_no_auth_path(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    # No new auth subtype was added.
    forbidden = (
        "subtype === 'login'",
        "subtype === 'logout'",
        "subtype === 'auth_login'",
        "subtype === 'auth_logout'",
        "subtype === 'apply_credential'",
        "subtype === 'set_credential'",
    )
    for token in forbidden:
        if token in src:
            fail(failures, f"不允许新增 auth 协议: {token}")


def check_slash_blocked_unchanged(failures: list[str]) -> None:
    src = CAPABILITIES_TS.read_text()
    for slash_id in ("slash.compact", "slash.config", "slash.doctor",
                     "slash.diff", "slash.ide", "slash.login", "slash.logout"):
        idx = src.find(f"id: '{slash_id}'")
        if idx < 0:
            fail(failures, f"manifest 缺 {slash_id}")
            continue
        window = src[idx : idx + 1500]
        if "status: 'blocked'" not in window:
            fail(failures, f"{slash_id} 必须仍 blocked")


def check_run_all_registration(failures: list[str]) -> None:
    src = RUN_ALL.read_text()
    if "wave_w47_real_capability_operations_smoke" not in src:
        fail(failures, "run_all_smoke.sh 未接入 W47 smoke")


def main() -> int:
    failures: list[str] = []
    check_schemas(failures)
    check_whitelist(failures)
    check_dispatch_apply_config_change(failures)
    check_dispatch_get_capability_operations(failures)
    check_dispatch_project_memory(failures)
    check_git_diff_patch_preview(failures)
    check_doctor_probe_extension(failures)
    check_compact_still_blocked(failures)
    check_no_auth_path(failures)
    check_slash_blocked_unchanged(failures)
    check_run_all_registration(failures)

    print("=== W47 real capability operations smoke ===")
    print(f"controlSchemas.ts: {CONTROL_SCHEMAS.relative_to(ROOT)}")
    print(f"print.ts:          {PRINT_TS.relative_to(ROOT)}")
    print(f"whitelist:         {WHITELIST.relative_to(ROOT)}")
    print(f"capabilities.ts:   {CAPABILITIES_TS.relative_to(ROOT)}")
    print(f"new subtypes:      {', '.join(NEW_SUBTYPES)}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W47 real capability operations ✓ "
        "(get_capability_operations real, git_diff patch preview real, "
        "doctor probe schema, apply_config_change blocked, "
        "project_memory_operation blocked, compact still blocked, no auth)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
