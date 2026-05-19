#!/usr/bin/env python3
"""
W46 — high-value dedicated control_request protocol contracts.

Locks the 4 new control_request subtypes added for the previously-blocked
high-value capabilities (compact / config / doctor / diff) plus the IDE
reuse of the existing mcp_status protocol:

  1. controlSchemas.ts declares all 4 new request schemas + responses.
  2. SDKControlRequestInner union has 26 members (was 22).
  3. Whitelist Section B lists the 4 new schemas.
  4. cli/print.ts dispatches each subtype with the documented contract:
     - compact_conversation: blocked-with-reason; never imports
       compactConversation; never sends success with status='completed'
       (until a follow-up wave designs idle ToolUseContext orchestration).
     - get_config_summary: never echoes setting values; only top-level
       key names + per-source counts.
     - runtime_doctor_summary: structured checks; no network/auth probes;
       no spawn calls.
     - git_diff_summary: bounded subprocess (5s timeout, 200-file cap);
       no patch echo; non-git fallback.
  5. Slash manifest reasons for compact/config/doctor/diff/ide point to
     the dedicated protocols by name.
  6. run_all_smoke.sh registers W46.
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
    "compact_conversation",
    "get_config_summary",
    "runtime_doctor_summary",
    "git_diff_summary",
)

NEW_SCHEMA_NAMES = (
    "SDKControlCompactConversationRequestSchema",
    "SDKControlCompactConversationResponseSchema",
    "SDKControlGetConfigSummaryRequestSchema",
    "SDKControlGetConfigSummaryResponseSchema",
    "SDKControlRuntimeDoctorSummaryRequestSchema",
    "SDKControlRuntimeDoctorSummaryResponseSchema",
    "SDKControlGitDiffSummaryRequestSchema",
    "SDKControlGitDiffSummaryResponseSchema",
)


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def slash_branch() -> str:
    src = PRINT_TS.read_text()
    match = re.search(
        r"message\.request\.subtype === 'slash_command'\)\s*\{([\s\S]*)\}\s*else\s*\{[\s\S]*?Unknown control request subtype",
        src,
    )
    return match.group(1) if match else ""


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
    # Union must include the 4 new request schemas.
    union_match = re.search(
        r"SDKControlRequestInnerSchema\s*=\s*lazySchema\(\(\)\s*=>\s*\n?\s*z\.union\(\[([\s\S]*?)\]\)",
        src,
    )
    if not union_match:
        fail(failures, "找不到 SDKControlRequestInnerSchema union 体")
        return
    union_body = union_match.group(1)
    for name in (
        "SDKControlCompactConversationRequestSchema",
        "SDKControlGetConfigSummaryRequestSchema",
        "SDKControlRuntimeDoctorSummaryRequestSchema",
        "SDKControlGitDiffSummaryRequestSchema",
    ):
        if name not in union_body:
            fail(failures, f"SDKControlRequestInner union 缺成员: {name}")


def check_whitelist(failures: list[str]) -> None:
    src = WHITELIST.read_text()
    if "Section B — SDKControlRequestInner union (29 成员" not in src:
        fail(failures, "whitelist Section B header 未更新到 29 成员")
    for name in (
        "SDKControlCompactConversationRequestSchema",
        "SDKControlGetConfigSummaryRequestSchema",
        "SDKControlRuntimeDoctorSummaryRequestSchema",
        "SDKControlGitDiffSummaryRequestSchema",
    ):
        if name not in src:
            fail(failures, f"whitelist 缺 Section B 成员: {name}")


def check_dispatch_compact(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'compact_conversation'",
        "subtype === 'get_config_summary'",
    )
    if not block:
        fail(failures, "缺 compact_conversation 分支")
        return
    # Only check code lines for forbidden tokens (skip comments)
    code_lines = [
        line for line in block.splitlines()
        if not line.strip().startswith("//") and not line.strip().startswith("*")
    ]
    code_text = "\n".join(code_lines)
    if "compactConversation" in code_text:
        fail(failures, "compact_conversation 分支不得调用 compactConversation")
    if "status: 'completed'" in block:
        fail(failures, "compact_conversation 仍必须 blocked，不得返回 status:'completed'")
    if "status: 'blocked'" not in block:
        fail(failures, "compact_conversation 必须返回 status:'blocked'")
    if "reason:" not in block:
        fail(failures, "compact_conversation blocked 必须带 reason")


def check_dispatch_config_summary(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'get_config_summary'",
        "subtype === 'runtime_doctor_summary'",
    )
    if not block:
        fail(failures, "缺 get_config_summary 分支")
        return
    required = (
        "getSettingsWithSources()",
        "topLevelKeys",
        "effectiveTopLevelKeys",
        "keyCount",
    )
    for token in required:
        if token not in block:
            fail(failures, f"get_config_summary 分支缺锚点: {token}")
    forbidden = (
        "JSON.stringify",  # would risk dumping values into the wire
        ".values(",
        "settings: entry.settings",
        "effective: withSources.effective",
    )
    for token in forbidden:
        if token in block:
            fail(
                failures,
                f"get_config_summary 分支不应回传 setting 值: {token}",
            )


def check_dispatch_doctor(failures: list[str]) -> None:
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
        "checks:",
        "buildMcpServerStatuses()",
        "getMemoryFiles()",
        "getAllHooks(",
        "id: 'cwd'",
        "id: 'session'",
        "id: 'model'",
        "id: 'permission_mode'",
        "id: 'mcp'",
        "id: 'memory'",
        "id: 'hooks'",
    )
    for token in required:
        if token not in block:
            fail(failures, f"runtime_doctor_summary 分支缺 check id: {token}")
    forbidden = (
        "fetch(",
        "https://",
        "http://",
        "spawn(",
        "execSync(",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"runtime_doctor_summary 分支不得做网络/spawn: {token}")


def check_dispatch_diff(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'git_diff_summary'",
        "subtype === 'slash_command'",
    )
    if not block:
        fail(failures, "缺 git_diff_summary 分支")
        return
    required = (
        "TIMEOUT_MS = 5000",
        "FILE_CAP = 200",
        "rev-parse",
        "--is-inside-work-tree",
        "git ${args[0]}",
        "not_git_repo",
        "GIT_OPTIONAL_LOCKS",
        "stdio: ['ignore', 'pipe', 'pipe']",
    )
    for token in required:
        if token not in block:
            fail(failures, f"git_diff_summary 分支缺锚点: {token}")
    # `patch:` and patch-preview safety are W47's territory — W47 enforces
    # PATCH_BYTE_CAP / PATCH_FILE_CAP / numstat binary detection. W46 only
    # checks that no git-mutation flag leaks here.
    forbidden = (
        "git push",
        "git commit",
        "git checkout",
        "git reset",
        "git apply",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"git_diff_summary 分支不得写 git/echo patch: {token}")


def check_manifest_reasons(failures: list[str]) -> None:
    src = CAPABILITIES_TS.read_text()
    expected_reasons = {
        "slash.compact": "compact_conversation",
        "slash.config": "get_config_summary",
        "slash.doctor": "runtime_doctor_summary",
        "slash.diff": "git_diff_summary",
        "slash.ide": "mcp_status",
    }
    for slash_id, dedicated in expected_reasons.items():
        anchor = f"id: '{slash_id}'"
        idx = src.find(anchor)
        if idx < 0:
            fail(failures, f"manifest 缺 entry: {slash_id}")
            continue
        # window-scan the entry body
        window = src[idx : idx + 1500]
        if dedicated not in window:
            fail(
                failures,
                f"{slash_id} reason 必须指向 dedicated protocol `{dedicated}`",
            )


def check_run_all_registration(failures: list[str]) -> None:
    src = RUN_ALL.read_text()
    if "wave_w46_high_value_protocols_smoke" not in src:
        fail(failures, "run_all_smoke.sh 未接入 W46 high-value protocols smoke")


def main() -> int:
    failures: list[str] = []
    check_schemas(failures)
    check_whitelist(failures)
    check_dispatch_compact(failures)
    check_dispatch_config_summary(failures)
    check_dispatch_doctor(failures)
    check_dispatch_diff(failures)
    check_manifest_reasons(failures)
    check_run_all_registration(failures)

    print("=== W46 high-value control protocols smoke ===")
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
        "PASS: 4 high-value dedicated protocols ✓ "
        "(compact_conversation blocked, get_config_summary redacted, "
        "runtime_doctor_summary structured, git_diff_summary bounded)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
