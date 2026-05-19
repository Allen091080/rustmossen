#!/usr/bin/env python3
"""
W49 — compact lifecycle event contract smoke.

Verifies the compact lifecycle events that Workbench consumes are
schema-locked and correctly wired in the SDKMessage union.

Lifecycle events audited:

  1. status: 'compacting'
     - SDKStatusSchema declares z.union([z.literal('compacting'), z.null()]).
     - compactConversation() calls context.setSDKStatus?.('compacting') before
       compact starts, and context.setSDKStatus?.(null) in its finally block.
     - commands/compact/compact.ts reactive handler does the same.
     - setSDKStatus produces an SDKStatusMessage enqueued to stdout.

  2. compact_boundary
     - SDKCompactBoundaryMessageSchema declares type='system',
       subtype='compact_boundary', with compact_metadata (trigger,
       pre_tokens, preserved_segment).
     - compactConversation() calls createCompactBoundaryMessage() after
       compaction and returns it as the boundary marker.
     - QueryEngine yields SDKCompactBoundaryMessage to the SDK stream.

  3. compact_completed (W48 system event)
     - SDKCompactCompletedEventSchema already verified by W48 smoke.
     - This smoke confirms it remains in the SDKMessage union and
       protocol-contract.md member count matches.

STOP conditions:
  - Does NOT modify compactConversation() semantics.
  - Does NOT add new compact execution paths.
  - Does NOT change auto compact threshold.
  - Does NOT touch query loop control flow.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORE_SCHEMAS = ROOT / "entrypoints" / "sdk" / "coreSchemas.ts"
COMPACT_TS = ROOT / "services" / "compact" / "compact.ts"
COMPACT_CMD_TS = ROOT / "commands" / "compact" / "compact.ts"
PROTOCOL_CONTRACT = ROOT / "docs" / "reference" / "protocol-contract.md"
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


def check_status_schema(failures: list[str]) -> None:
    src = CORE_SCHEMAS.read_text()
    if "export const SDKStatusSchema" not in src:
        fail(failures, "coreSchemas.ts 缺 SDKStatusSchema 导出")
        return
    block = section(src, "export const SDKStatusSchema", "// SDKUserMessage")
    if "z.literal('compacting')" not in block:
        fail(failures, "SDKStatusSchema 缺 z.literal('compacting')")
    if "z.null()" not in block:
        fail(failures, "SDKStatusSchema 缺 z.null()")


def check_status_message_schema(failures: list[str]) -> None:
    src = CORE_SCHEMAS.read_text()
    block = section(
        src,
        "export const SDKStatusMessageSchema",
        "export const SDKPostTurnSummaryMessageSchema",
    )
    if not block:
        fail(failures, "缺 SDKStatusMessageSchema")
        return
    required = (
        "subtype: z.literal('status')",
        "status: SDKStatusSchema()",
        "session_id: z.string()",
    )
    for token in required:
        if token not in block:
            fail(failures, f"SDKStatusMessageSchema 缺锚点: {token}")


def check_boundary_schema(failures: list[str]) -> None:
    src = CORE_SCHEMAS.read_text()
    block = section(
        src,
        "export const SDKCompactBoundaryMessageSchema",
        "export const SDKCompactCompletedEventSchema",
    )
    if not block:
        fail(failures, "缺 SDKCompactBoundaryMessageSchema")
        return
    required = (
        "subtype: z.literal('compact_boundary')",
        "trigger: z.enum(['manual', 'auto'])",
        "pre_tokens: z.number()",
        "preserved_segment",
        "head_uuid",
        "anchor_uuid",
        "tail_uuid",
        "session_id: z.string()",
    )
    for token in required:
        if token not in block:
            fail(failures, f"SDKCompactBoundaryMessageSchema 缺锚点: {token}")


def check_compacting_emit_sites(failures: list[str]) -> None:
    """Verify compactConversation emits 'compacting' and clears it."""
    src = COMPACT_TS.read_text()
    # Must emit compacting
    if "context.setSDKStatus?.('compacting')" not in src:
        fail(failures, "compact.ts 缺 setSDKStatus('compacting') 调用")
    # Must clear (null)
    if "context.setSDKStatus?.(null)" not in src:
        fail(failures, "compact.ts 缺 setSDKStatus(null) 清除")


def check_compact_cmd_emit_sites(failures: list[str]) -> None:
    """Verify reactive /compact command emits 'compacting' and clears it."""
    src = COMPACT_CMD_TS.read_text()
    if "context.setSDKStatus?.('compacting')" not in src:
        fail(
            failures,
            "commands/compact/compact.ts 缺 setSDKStatus('compacting') 调用",
        )
    if "context.setSDKStatus?.(null)" not in src:
        fail(failures, "commands/compact/compact.ts 缺 setSDKStatus(null) 清除")


def check_boundary_creation(failures: list[str]) -> None:
    """Verify compactConversation creates boundary marker."""
    src = COMPACT_TS.read_text()
    if "createCompactBoundaryMessage" not in src:
        fail(failures, "compact.ts 缺 createCompactBoundaryMessage 调用")


def check_sdk_message_union(failures: list[str]) -> None:
    """Verify SDKBoundary and SDKStatus schemas are in SDKMessage union."""
    src = CORE_SCHEMAS.read_text()
    union_match = re.search(
        r"SDKMessageSchema\s*=\s*lazySchema\(\(\)\s*=>\s*\n?\s*z\.union\(\[([\s\S]*?)\]\)",
        src,
    )
    if not union_match:
        fail(failures, "找不到 SDKMessageSchema union 体")
        return
    union_body = union_match.group(1)
    for name in (
        "SDKCompactBoundaryMessageSchema",
        "SDKCompactCompletedEventSchema",
        "SDKStatusMessageSchema",
    ):
        if name not in union_body:
            fail(failures, f"SDKMessage union 缺成员: {name}")


def check_protocol_contract_count(failures: list[str]) -> None:
    """Verify protocol-contract.md §2.2 says 28 members."""
    src = PROTOCOL_CONTRACT.read_text()
    if "SDKMessage union (28 成员)" not in src:
        fail(
            failures,
            "protocol-contract.md §2.2 未更新到 28 成员",
        )
    if "SDKCompactCompletedEvent" not in src:
        fail(
            failures,
            "protocol-contract.md §2.2 成员列表缺 SDKCompactCompletedEvent",
        )


def check_run_all_registration(failures: list[str]) -> None:
    src = RUN_ALL.read_text()
    if "wave_w49_compact_lifecycle_event_smoke" not in src:
        fail(failures, "run_all_smoke.sh 未接入 W49 smoke")


def main() -> int:
    failures: list[str] = []
    check_status_schema(failures)
    check_status_message_schema(failures)
    check_boundary_schema(failures)
    check_compacting_emit_sites(failures)
    check_compact_cmd_emit_sites(failures)
    check_boundary_creation(failures)
    check_sdk_message_union(failures)
    check_protocol_contract_count(failures)
    check_run_all_registration(failures)

    print("=== W49 compact lifecycle event smoke ===")
    print(f"coreSchemas.ts:   {CORE_SCHEMAS.relative_to(ROOT)}")
    print(f"compact.ts:       {COMPACT_TS.relative_to(ROOT)}")
    print(f"compact cmd:      {COMPACT_CMD_TS.relative_to(ROOT)}")
    print(f"protocol-contract: {PROTOCOL_CONTRACT.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W49 compact lifecycle events ✓ "
        "(SDKStatusSchema has 'compacting'/null, "
        "SDKStatusMessageSchema wired, "
        "SDKCompactBoundaryMessageSchema has trigger/pre_tokens/preserved_segment, "
        "compactConversation emits 'compacting' + clears + creates boundary, "
        "reactive /compact emits 'compacting' + clears, "
        "SDKMessage union has all 3 schemas, "
        "protocol-contract.md §2.2 at 28 members)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
