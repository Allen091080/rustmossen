#!/usr/bin/env python3
"""
W48 — compact_conversation queued protocol smoke.

Locks the W48 protocol additions for compact_conversation:

  Queued protocol (handler side):
    - control_request handler validates mode/custom_instructions and
      enqueues into pendingCompactRequest.ts single-slot buffer.
    - Returns status="queued" on success, status="blocked" on rejection.
    - Never calls compactConversation.
    - Never constructs ToolUseContext.
    - Never constructs CacheSafeParams.

  Event schema (query loop side, not yet wired):
    - coreSchemas.ts has SDKCompactCompletedEventSchema (system event).
    - Event carries request_id + compact_result (completed/failed).

  Pending buffer:
    - pendingCompactRequest.ts provides single-slot enqueue/dequeue.
    - No execution logic in the buffer module.

  STOP conditions:
    - query.ts has no new compact execution path this wave.
    - compactConversation is not imported in print.ts compact branch.
    - slash /compact remains blocked in slash_command path.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PRINT_TS = ROOT / "cli" / "print.ts"
CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
CORE_SCHEMAS = ROOT / "entrypoints" / "sdk" / "coreSchemas.ts"
CAPABILITIES_TS = ROOT / "src" / "services" / "slashCommandCapabilities.ts"
PENDING_TS = ROOT / "services" / "compact" / "pendingCompactRequest.ts"
QUERY_TS = ROOT / "query.ts"
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


def check_request_schema(failures: list[str]) -> None:
    src = CONTROL_SCHEMAS.read_text()
    if "custom_instructions:" not in section(
        src, "SDKControlCompactConversationRequestSchema",
        "SDKControlCompactConversationResponseSchema",
    ):
        fail(failures, "request schema 缺 custom_instructions 字段")


def check_response_schema(failures: list[str]) -> None:
    src = CONTROL_SCHEMAS.read_text()
    resp = section(
        src,
        "SDKControlCompactConversationResponseSchema",
        "SDKControlGetConfigSummaryRequestSchema",
    )
    for status in ("queued", "blocked", "completed", "failed"):
        if f"'{status}'" not in resp:
            fail(failures, f"response schema 缺 status '{status}'")


def check_pending_buffer(failures: list[str]) -> None:
    if not PENDING_TS.exists():
        fail(failures, "pendingCompactRequest.ts 不存在")
        return
    src = PENDING_TS.read_text()
    required = (
        "PendingCompactRequest",
        "enqueuePendingCompactRequest",
        "getPendingCompactRequest",
        "clearPendingCompactRequest",
        "hasPendingCompactRequest",
    )
    for token in required:
        if token not in src:
            fail(failures, f"pendingCompactRequest.ts 缺 {token}")
    # Must NOT contain compact execution logic in code lines (skip comments)
    code_lines = [
        line for line in src.splitlines()
        if not line.strip().startswith("//") and not line.strip().startswith("*")
    ]
    code_text = "\n".join(code_lines)
    forbidden = (
        "compactConversation(",
        "ToolUseContext",
        "CacheSafeParams",
    )
    for token in forbidden:
        if token in code_text:
            fail(
                failures,
                f"pendingCompactRequest.ts 代码行不应包含 {token}",
            )


def check_compact_completed_event(failures: list[str]) -> None:
    src = CORE_SCHEMAS.read_text()
    if "SDKCompactCompletedEventSchema" not in src:
        fail(failures, "coreSchemas.ts 缺 SDKCompactCompletedEventSchema")
        return
    event_block = section(
        src,
        "SDKCompactCompletedEventSchema",
        "SDKStatusMessageSchema",
    )
    required = (
        "compact_completed",
        "request_id",
        "compact_result",
        "completed",
        "failed",
        "reason",
        "preCompactTokenCount",
        "postCompactTokenCount",
        "messageCountBefore",
        "messageCountAfter",
    )
    for token in required:
        if token not in event_block:
            fail(
                failures,
                f"SDKCompactCompletedEventSchema 缺字段: {token}",
            )
    # Must be in SDKMessage union
    union_match = re.search(
        r"SDKMessageSchema\s*=\s*lazySchema\(\(\)\s*=>\s*\n?\s*z\.union\(\[([\s\S]*?)\]\)",
        src,
    )
    if union_match and "SDKCompactCompletedEventSchema" not in union_match.group(1):
        fail(failures, "SDKMessage union 缺 SDKCompactCompletedEventSchema")


def check_dispatcher_queued(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    block = section(
        src,
        "subtype === 'compact_conversation'",
        "subtype === 'get_config_summary'",
    )
    if not block:
        fail(failures, "缺 compact_conversation 分支")
        return
    # Must have queued path
    if "status: 'queued'" not in block:
        fail(failures, "compact_conversation 分支缺 queued path")
    # Must have blocked path
    if "status: 'blocked'" not in block:
        fail(failures, "compact_conversation 分支缺 blocked path")
    # Must enqueue
    if "enqueuePendingCompactRequest" not in block:
        fail(failures, "compact_conversation 分支缺 enqueue 调用")
    # Must NOT call compactConversation / construct ToolUseContext / CacheSafeParams
    # in code lines (skip comments)
    code_lines = [
        line for line in block.splitlines()
        if not line.strip().startswith("//") and not line.strip().startswith("*")
    ]
    code_text = "\n".join(code_lines)
    forbidden = (
        "compactConversation(",
        "ToolUseContext",
        "CacheSafeParams",
    )
    for token in forbidden:
        if token in code_text:
            fail(
                failures,
                f"compact_conversation 分支代码行不得包含 {token}",
            )
    # Must validate mode
    if "mode !== 'manual'" not in block:
        fail(failures, "compact_conversation 分支缺 mode 验证")
    # Must handle dry_run
    if "dryRun" not in block:
        fail(failures, "compact_conversation 分支缺 dry_run 处理")


def check_query_ts_untouched(failures: list[str]) -> None:
    src = QUERY_TS.read_text()
    if "dequeuePendingCompactRequest" in src:
        fail(
            failures,
            "query.ts 不应在本轮新增 compact execution path",
        )


def check_slash_compact_still_blocked(failures: list[str]) -> None:
    src = CAPABILITIES_TS.read_text()
    idx = src.find("id: 'slash.compact'")
    if idx < 0:
        fail(failures, "manifest 缺 slash.compact")
        return
    window = src[idx : idx + 1500]
    if "status: 'blocked'" not in window:
        fail(failures, "slash.compact 必须仍 blocked")


def check_run_all_registration(failures: list[str]) -> None:
    src = RUN_ALL.read_text()
    if "wave_w48_compact_queued_protocol_smoke" not in src:
        fail(failures, "run_all_smoke.sh 未接入 W48 smoke")


def main() -> int:
    failures: list[str] = []
    check_request_schema(failures)
    check_response_schema(failures)
    check_pending_buffer(failures)
    check_compact_completed_event(failures)
    check_dispatcher_queued(failures)
    check_query_ts_untouched(failures)
    check_slash_compact_still_blocked(failures)
    check_run_all_registration(failures)

    print("=== W48 compact queued protocol smoke ===")
    print(f"controlSchemas.ts: {CONTROL_SCHEMAS.relative_to(ROOT)}")
    print(f"coreSchemas.ts:    {CORE_SCHEMAS.relative_to(ROOT)}")
    print(f"print.ts:          {PRINT_TS.relative_to(ROOT)}")
    print(f"pending:           {PENDING_TS.relative_to(ROOT)}")
    print(f"query.ts:          {QUERY_TS.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W48 compact queued protocol ✓ "
        "(request schema extended, response has queued/blocked/completed/failed, "
        "pending buffer single-slot, compact_completed event schema, "
        "dispatcher enqueues, no compactConversation/ToolUseContext/CacheSafeParams "
        "in handler, query.ts untouched, slash.compact still blocked)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
