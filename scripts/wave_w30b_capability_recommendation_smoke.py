#!/usr/bin/env python3
"""W30-B — capability_recommendation protocol extension smoke (静态校验).

契约 (本 slice 必须保住):
  Schema 层
    1. SDKCapabilityRecommendationMessageSchema + SDKCapabilityRecommendationResultMessageSchema 存在并入 SDKMessageSchema union
    2. Event 字段: type='capability_recommendation', recommendation_id, capability, trigger, choices, uuid, session_id
    3. capability.id 用 namespace 格式 (schema comment 说明)
    4. trigger.kind 仅 'file_extension' (本轮), trigger.value 是扩展名而非全路径
    5. choices 恰好 4 项, choice.id 固定: install / not_now / never_for_capability / disable_all_recommendations
    6. SDKCapabilityRecommendationResponseSchema 存在并入 StdinMessageSchema union
    7. Response 字段: type='capability_recommendation_response', recommendation_id, choice_id
  Safety 层
    8. emit 函数不调用 installPluginAndNotify / addToNeverSuggest / saveGlobalConfig
    9. structuredIO processLine 处理 capability_recommendation_response via lspRecommendation helper
   10. response helper 显式 install 才调用 installResolvedPlugin，不走 TUI installPluginAndNotify
   11. 不泄漏本地路径: trigger.value 不含 '/' 或 '\\' (仅扩展名如 ".ts")
  Builder 层
   12. buildCapabilityRecommendationEvent 函数存在于 lspRecommendation.ts
   13. CAPABILITY_RECOMMENDATION_CHOICES 常量含 4 固定选项
  Whitelist 层
   14. whitelist Section A 含 capability recommendation schemas, count 27
   15. whitelist Section D 含 SDKCapabilityRecommendationResponseSchema, count 6

跑法:
  python3 scripts/wave_w30b_capability_recommendation_smoke.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORE_SCHEMAS = ROOT / "entrypoints" / "sdk" / "coreSchemas.ts"
CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
LSP_RECOMMENDATION = ROOT / "utils" / "plugins" / "lspRecommendation.ts"
STRUCTURED_IO = ROOT / "cli" / "structuredIO.ts"
PRINT_TS = ROOT / "cli" / "print.ts"
WHITELIST = ROOT / "scripts" / "stream-json-schema-whitelist.txt"

EXPECTED_CHOICE_IDS = {
    "install",
    "not_now",
    "never_for_capability",
    "disable_all_recommendations",
}


def fail(msgs: list[str], msg: str) -> None:
    msgs.append(msg)


def check_event_schema(failures: list[str]) -> None:
    src = CORE_SCHEMAS.read_text()

    # 1. Name exists
    if "export const SDKCapabilityRecommendationMessageSchema" not in src:
        fail(failures, "SDKCapabilityRecommendationMessageSchema 未定义")
        return

    # 2. Field contract
    block = _extract_schema_block(src, "SDKCapabilityRecommendationMessageSchema")
    if not block:
        fail(failures, "SDKCapabilityRecommendationMessageSchema 块结构异常")
        return
    for field in (
        "type: z.literal('capability_recommendation')",
        "recommendation_id:",
        "capability:",
        "trigger:",
        "choices:",
        "uuid:",
        "session_id:",
    ):
        if field not in block:
            fail(failures, f"SDKCapabilityRecommendationMessageSchema 缺字段: {field}")

    # 3. capability.id namespace hint (in CapabilityInfoSchema, not the message schema)
    cap_info_block = _extract_schema_block(src, "CapabilityInfoSchema")
    if cap_info_block and "lsp.typescript-lsp" not in cap_info_block:
        fail(failures, "CapabilityInfoSchema capability.id 缺 namespace 示例 (lsp.typescript-lsp)")

    # 4. trigger safety — kind is file_extension only
    trigger_block = _extract_sub_schema(block, "CapabilityRecommendationTriggerSchema")
    if trigger_block:
        if "file_extension" not in trigger_block:
            fail(failures, "trigger.kind 缺 file_extension")

    # 5. choices structure — check CapabilityRecommendationChoiceSchema
    choice_block = _extract_sub_schema(src, "CapabilityRecommendationChoiceSchema")
    if choice_block:
        for expected in ("id:", "label:", "kind:"):
            if expected not in choice_block:
                fail(failures, f"CapabilityRecommendationChoiceSchema 缺: {expected}")

    # 6. Union membership
    union_match = re.search(
        r"SDKMessageSchema = lazySchema\([\s\S]*?z\.union\(\[([\s\S]*?)\]\)",
        src,
    )
    if not union_match:
        fail(failures, "SDKMessageSchema union 块抓取失败")
        return
    union_body = union_match.group(1)
    if "SDKCapabilityRecommendationMessageSchema()" not in union_body:
        fail(failures, "SDKCapabilityRecommendationMessageSchema() 未加入 SDKMessageSchema union")
    if "SDKCapabilityRecommendationResultMessageSchema()" not in union_body:
        fail(failures, "SDKCapabilityRecommendationResultMessageSchema() 未加入 SDKMessageSchema union")

    if "export const SDKCapabilityRecommendationResultMessageSchema" not in src:
        fail(failures, "SDKCapabilityRecommendationResultMessageSchema 未定义")
    else:
        result_block = _extract_schema_block(src, "SDKCapabilityRecommendationResultMessageSchema")
        if not result_block:
            fail(failures, "SDKCapabilityRecommendationResultMessageSchema 块结构异常")
        else:
            for field in (
                "type: z.literal('capability_recommendation_result')",
                "recommendation_id:",
                "choice_id:",
                "action:",
                "status:",
                "summary:",
                "uuid:",
                "session_id:",
            ):
                if field not in result_block:
                    fail(failures, f"SDKCapabilityRecommendationResultMessageSchema 缺字段: {field}")


def check_response_schema(failures: list[str]) -> None:
    src = CONTROL_SCHEMAS.read_text()

    # 6. Name exists
    if "export const SDKCapabilityRecommendationResponseSchema" not in src:
        fail(failures, "SDKCapabilityRecommendationResponseSchema 未定义")
        return

    block = _extract_schema_block(src, "SDKCapabilityRecommendationResponseSchema")
    if not block:
        fail(failures, "SDKCapabilityRecommendationResponseSchema 块结构异常")
        return

    # 7. Field contract
    for field in (
        "type: z.literal('capability_recommendation_response')",
        "recommendation_id:",
        "choice_id:",
    ):
        if field not in block:
            fail(failures, f"SDKCapabilityRecommendationResponseSchema 缺字段: {field}")

    if "handles explicit install" not in block:
        fail(failures, "SDKCapabilityRecommendationResponseSchema 缺 explicit install handling 说明")

    # Union membership in StdinMessage
    stdin_match = re.search(
        r"StdinMessageSchema = lazySchema\([\s\S]*?z\.union\(\[([\s\S]*?)\]\)",
        src,
    )
    if not stdin_match:
        fail(failures, "StdinMessageSchema union 块抓取失败")
        return
    if "SDKCapabilityRecommendationResponseSchema()" not in stdin_match.group(1):
        fail(failures, "SDKCapabilityRecommendationResponseSchema() 未加入 StdinMessageSchema union")


def check_safety_emit(failures: list[str]) -> None:
    """Emit path must not call install/config/side-effect functions."""
    src = PRINT_TS.read_text()

    # Find the emit helper
    if "emitCapabilityRecommendationsForCwd" not in src:
        fail(failures, "print.ts 缺 emitCapabilityRecommendationsForCwd 函数")
        return

    # Extract the function body
    func_match = re.search(
        r"async function emitCapabilityRecommendationsForCwd\(([\s\S]*?)\n\}",
        src,
    )
    if not func_match:
        fail(failures, "emitCapabilityRecommendationsForCwd 函数体抓取失败")
        return
    body = func_match.group(1)

    # 8. No side-effect calls
    for forbidden in (
        "installPluginAndNotify",
        "addToNeverSuggest",
        "saveGlobalConfig",
        "cacheAndRegisterPlugin",
    ):
        if forbidden in body:
            fail(failures, f"emitCapabilityRecommendationsForCwd 禁止调用: {forbidden}")

    # Must use buildCapabilityRecommendationEvent (not manual construction)
    if "buildCapabilityRecommendationEvent" not in body:
        fail(failures, "emitCapabilityRecommendationsForCwd 必须使用 buildCapabilityRecommendationEvent")


def check_safety_response(failures: list[str]) -> None:
    """structuredIO must route response through the central LSP helper."""
    src = STRUCTURED_IO.read_text()

    if "capability_recommendation_response" not in src:
        fail(failures, "structuredIO.ts 未处理 capability_recommendation_response")
        return

    # Find the handler block
    handler_match = re.search(
        r"message\.type === 'capability_recommendation_response'([\s\S]*?)return undefined",
        src,
    )
    if not handler_match:
        fail(failures, "capability_recommendation_response handler 块抓取失败")
        return
    body = handler_match.group(1)

    # 9. No direct side effects in structuredIO; delegate to helper.
    if "handleCapabilityRecommendationResponse" not in body:
        fail(failures, "capability_recommendation_response handler 未调用 handleCapabilityRecommendationResponse")
    for forbidden in (
        "installPluginAndNotify",
        "addToNeverSuggest",
        "saveGlobalConfig",
        "cacheAndRegisterPlugin",
    ):
        if forbidden in body:
            fail(failures, f"capability_recommendation_response handler 禁止直接调用: {forbidden}")

    if "await handleCapabilityRecommendationResponse" not in body:
        fail(failures, "capability_recommendation_response handler 必须 await async helper")
    if "capability_recommendation_result" not in body:
        fail(failures, "capability_recommendation_response handler 必须返回 result event")


def check_response_helper_install(failures: list[str]) -> None:
    """Install side effects must live in the centralized helper and only under explicit install."""
    src = LSP_RECOMMENDATION.read_text()

    if "export async function handleCapabilityRecommendationResponse" not in src:
        fail(failures, "handleCapabilityRecommendationResponse 必须是 async")
        return

    body = src.split("export async function handleCapabilityRecommendationResponse", 1)[1]

    for expected in (
        "choiceId === 'install'",
        "getPluginById(pluginId)",
        "installResolvedPlugin",
        "scope: 'user'",
        "marketplaceInstallLocation: pluginInfo.marketplaceInstallLocation",
        "capabilityRecommendationPluginIds.delete(recommendationId)",
        "'installed'",
        "'install_failed'",
        "'install_not_found'",
    ):
        if expected not in body:
            fail(failures, f"install response helper 缺少: {expected}")

    if "installPluginAndNotify" in body:
        fail(failures, "stream-json install 禁止调用 TUI installPluginAndNotify")

    install_branch = body.split("choiceId === 'install'", 1)[-1]
    before_install_branch = body.split("choiceId === 'install'", 1)[0]
    if "installResolvedPlugin" in before_install_branch:
        fail(failures, "installResolvedPlugin 只能在 explicit install 分支内调用")
    if "installResolvedPlugin" not in install_branch:
        fail(failures, "explicit install 分支未调用 installResolvedPlugin")


def check_no_path_leak(failures: list[str]) -> None:
    """trigger.value must be extension only, not full path."""
    src = LSP_RECOMMENDATION.read_text()

    if "buildCapabilityRecommendationEvent" not in src:
        fail(failures, "lspRecommendation.ts 缺 buildCapabilityRecommendationEvent")
        return

    # Find the function
    func_match = re.search(
        r"function buildCapabilityRecommendationEvent\(([\s\S]*?)\n\}",
        src,
    )
    if not func_match:
        fail(failures, "buildCapabilityRecommendationEvent 函数体抓取失败")
        return
    body = func_match.group(1)

    # 10. trigger.value must use fileExtension parameter (not file path)
    if "value: fileExtension" not in body:
        fail(failures, "trigger.value 必须直接使用 fileExtension 参数 (不泄漏路径)")

    # Must NOT contain path manipulation for trigger value
    if "filePath" in body and "value:" in body:
        # Only fileExtension should be used in trigger
        lines_with_value = [l for l in body.splitlines() if "value:" in l]
        for line in lines_with_value:
            if "filePath" in line:
                fail(failures, f"trigger.value 行引用了 filePath (路径泄漏): {line.strip()}")


def check_builder(failures: list[str]) -> None:
    """Builder function and constants exist."""
    src = LSP_RECOMMENDATION.read_text()

    # 11. buildCapabilityRecommendationEvent
    if "export function buildCapabilityRecommendationEvent" not in src:
        fail(failures, "lspRecommendation.ts 缺 export function buildCapabilityRecommendationEvent")

    # 12. CAPABILITY_RECOMMENDATION_CHOICES constant with 4 entries
    choices_match = re.search(
        r"CAPABILITY_RECOMMENDATION_CHOICES\s*=\s*\[([\s\S]*?)\]",
        src,
    )
    if not choices_match:
        fail(failures, "CAPABILITY_RECOMMENDATION_CHOICES 常量缺失")
        return
    choices_body = choices_match.group(1)
    found_ids = set(re.findall(r"id:\s*'([^']+)'", choices_body))
    if found_ids != EXPECTED_CHOICE_IDS:
        missing = EXPECTED_CHOICE_IDS - found_ids
        extra = found_ids - EXPECTED_CHOICE_IDS
        if missing:
            fail(failures, f"CAPABILITY_RECOMMENDATION_CHOICES 缺少: {missing}")
        if extra:
            fail(failures, f"CAPABILITY_RECOMMENDATION_CHOICES 多余: {extra}")
    if len(found_ids) != 4:
        fail(failures, f"CAPABILITY_RECOMMENDATION_CHOICES 必须恰好 4 项, 实测 {len(found_ids)}")


def check_whitelist(failures: list[str]) -> None:
    src = WHITELIST.read_text()

    # 13. Section A
    if "SDKCapabilityRecommendationMessageSchema" not in src:
        fail(failures, "whitelist 漏 SDKCapabilityRecommendationMessageSchema")
    if "SDKCapabilityRecommendationResultMessageSchema" not in src:
        fail(failures, "whitelist 漏 SDKCapabilityRecommendationResultMessageSchema")
    if "Section A — SDKMessage union (27 成员" not in src:
        fail(failures, "whitelist Section A header 未升 27 成员")

    # 14. Section D
    if "SDKCapabilityRecommendationResponseSchema" not in src:
        fail(failures, "whitelist 漏 SDKCapabilityRecommendationResponseSchema")
    if "Section D — StdinMessage union (6 成员" not in src:
        fail(failures, "whitelist Section D header 未升 6 成员")


def check_tui_not_broken(failures: list[str]) -> None:
    """TUI LSP recommendation must still work."""
    # useLspPluginRecommendation should still exist and import getMatchingLspPlugins
    hook_path = ROOT / "hooks" / "useLspPluginRecommendation.tsx"
    if not hook_path.exists():
        fail(failures, "useLspPluginRecommendation.tsx 文件缺失 — TUI 被破坏")
        return
    src = hook_path.read_text()
    if "getMatchingLspPlugins" not in src:
        fail(failures, "useLspPluginRecommendation 不再 import getMatchingLspPlugins — TUI 被破坏")


def _extract_schema_block(src: str, name: str) -> str | None:
    """Extract a schema export block — captures everything from the lazySchema opening
    to the next `export const` or section comment or EOF."""
    m = re.search(
        rf"export const {re.escape(name)} = lazySchema\(([\s\S]*?)(?=\nexport const |\n// ={{3,}}|\Z)",
        src,
    )
    return m.group(1) if m else None


def _extract_sub_schema(src: str, name: str) -> str | None:
    """Extract a sub-schema block (same logic as _extract_schema_block)."""
    return _extract_schema_block(src, name)


def main() -> int:
    failures: list[str] = []
    check_event_schema(failures)
    check_response_schema(failures)
    check_safety_emit(failures)
    check_safety_response(failures)
    check_response_helper_install(failures)
    check_no_path_leak(failures)
    check_builder(failures)
    check_whitelist(failures)
    check_tui_not_broken(failures)

    print("=== W30-B capability_recommendation smoke ===")
    print(f"coreSchemas.ts:   {CORE_SCHEMAS.relative_to(ROOT)}")
    print(f"controlSchemas.ts:{CONTROL_SCHEMAS.relative_to(ROOT)}")
    print(f"lspRecommendation:{LSP_RECOMMENDATION.relative_to(ROOT)}")
    print(f"structuredIO.ts:  {STRUCTURED_IO.relative_to(ROOT)}")
    print(f"print.ts:         {PRINT_TS.relative_to(ROOT)}")
    print(f"whitelist:        {WHITELIST.relative_to(ROOT)}")
    print(f"expected choices: {', '.join(sorted(EXPECTED_CHOICE_IDS))}")

    if failures:
        print()
        print("=== FAIL ===")
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W30-B capability_recommendation ✓ "
        "(schema + event shape + response schema + explicit install helper + config choices handled + "
        "emit safety + no-path-leak + builder + whitelist 27/6 + TUI intact)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
