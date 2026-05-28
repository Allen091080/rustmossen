#!/usr/bin/env python3
"""
W44 — /cost stream-json slash wrapper contract smoke.

Verifies that the /cost slash bridge in cli/print.ts:
  1. Reads cost via cost-tracker getters (no LLM calls, no writes).
  2. Returns the documented payload shape under `cost.*`.
  3. Rejects extra args with `unsupported_slash_command_args: cost`.
  4. Does not mutate session/config/state — only reads.
  5. Is registered in the capability manifest with payload key `cost`.

Static-only smoke; mirrors the W42 wrapper smoke pattern. Runtime cost
behavior is owned by the cost-tracker test suite, not by this contract.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PRINT_TS = ROOT / "cli" / "print.ts"
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


def check_cost_branch(body: str, failures: list[str]) -> None:
    block = section(body, "command === 'cost'", "command === 'skills'")
    if not block:
        fail(failures, "缺 command === 'cost' 分支")
        return
    required = (
        "command: 'cost'",
        "subtype: 'slash_command_result'",
        "status: 'completed'",
        "summary:",
        "cost: {",
        "totalCostUsd",
        "getTotalCost()",
        "hasUnknownModelCost()",
        "getTotalDuration()",
        "getTotalAPIDuration()",
        "getTotalToolDuration()",
        "getTotalInputTokens()",
        "getTotalOutputTokens()",
        "getTotalCacheReadInputTokens()",
        "getTotalCacheCreationInputTokens()",
        "getTotalWebSearchRequests()",
        "getTotalLinesAdded()",
        "getTotalLinesRemoved()",
        "unsupported_slash_command_args: cost",
        "args.length",
    )
    for token in required:
        if token not in block:
            fail(failures, f"cost 分支缺契约锚点: {token}")
    forbidden = (
        "setTotalCost",
        "resetCost",
        "saveGlobalConfig",
        "writeFile",
        "compactConversation",
        "setMainLoopModelOverride",
        "setSessionActiveProfile",
        "applyFlagSettings",
    )
    for token in forbidden:
        if token in block:
            fail(failures, f"cost 分支不应有副作用调用: {token}")


def check_capability_manifest(failures: list[str]) -> None:
    src = CAPABILITIES_TS.read_text()
    if "command: 'cost'" not in src:
        fail(failures, "capability manifest 缺 command: 'cost'")
    cost_block_match = re.search(
        r"command:\s*'cost'[\s\S]*?\}\s*,\s*\{\s*command:",
        src,
    )
    cost_block = cost_block_match.group(0) if cost_block_match else ""
    if cost_block:
        if "readOnly: true" not in cost_block:
            fail(failures, "/cost manifest 必须 readOnly: true")
        if "sideEffect: 'none'" not in cost_block:
            fail(failures, "/cost manifest 必须 sideEffect: 'none'")
        if "requiresConfirmation: false" not in cost_block:
            fail(failures, "/cost manifest 必须 requiresConfirmation: false")


def check_run_all_registration(failures: list[str]) -> None:
    src = RUN_ALL.read_text()
    if "wave_w44_cost_slash_smoke" not in src:
        fail(failures, "run_all_smoke.sh 未接入 W44 cost slash smoke")


def main() -> int:
    failures: list[str] = []
    try:
        body = slash_branch()
    except RuntimeError as exc:
        failures.append(str(exc))
        body = ""

    if body:
        check_cost_branch(body, failures)
    check_capability_manifest(failures)
    check_run_all_registration(failures)

    print("=== W44 cost slash wrapper smoke ===")
    print(f"print.ts:        {PRINT_TS.relative_to(ROOT)}")
    print(f"capabilities.ts: {CAPABILITIES_TS.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print("PASS: /cost read-only slash wrapper ✓")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
