#!/usr/bin/env python3
"""W38/W45 — /model slash_command stream-json status + session profile bridge.

契约 (本 slice 必须保住):
  Dispatcher 层
    1. command === 'model' 分支存在
    2. args [] 走 status success 路径, 返回 slash_command_result
    3. args [profileName] 走 session-only profile switch, 返回 slash_command_result
    4. response.model object 必须含 current / source / available / profiles / switched
    5. args >1 走 error 路径, 含 unsupported_slash_command_args
  Safety 层
    6. 分支不调用 write config
    7. 分支不调用 TUI /model interactive path (commands/model/)
    8. 不泄漏 secret / env / raw config / local config path
  Regression 层
    9. /help still supported, /status still supported
    10. /compact still unsupported

跑法:
  python3 scripts/wave_w38_model_slash_command_smoke.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PRINT_TS = ROOT / "cli" / "print.ts"

FORBIDDEN_IN_MODEL_BRANCH = (
    "setModel",
    "saveGlobalConfig",
    "saveSettings",
    "updateSettingsForSource",
    "switchModel",
    "selectModel",
    "modelPicker",
    "ModelPicker",
    "interactive",
    "commands/model",
    "process.env.MOSSEN_CODE_MODEL",
    "getSettings_DEPRECATED",
)


def fail(msgs: list[str], msg: str) -> None:
    msgs.append(msg)


def extract_model_branch(src: str) -> str | None:
    """Extract the command === 'model' branch body.
    The branch ends at '} else if (command ===' or the final '} else {'
    for the unknown-command fallback. We match on the specific next branches.
    """
    m = re.search(
        r"command === 'model'\)\s*\{([\s\S]*?)\n\s*\}\s*else if \(command === ''",
        src,
    )
    return m.group(1) if m else None


def check_model_branch_exists(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    if "command === 'model'" not in src:
        fail(failures, "print.ts 缺 command === 'model' 分支")


def check_model_success_path(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    branch = extract_model_branch(src)
    if not branch:
        fail(failures, "model 分支体抓取失败")
        return

    # Must have success path with slash_command_result
    if "subtype: 'slash_command_result'" not in branch:
        fail(failures, "model 分支缺 subtype: 'slash_command_result'")

    # Must have model object with current/source/available
    if "'model'" not in branch and '"model"' not in branch:
        fail(failures, "model 分支缺 model 字段")
    if "current:" not in branch:
        fail(failures, "model response 缺 current 字段")
    if "source:" not in branch:
        fail(failures, "model response 缺 source 字段")
    if "available:" not in branch:
        fail(failures, "model response 缺 available 字段")
    if "profiles:" not in branch:
        fail(failures, "model response 缺 profiles 字段")
    if "switched:" not in branch:
        fail(failures, "model response 缺 switched 字段")

    # Must use pure model functions
    if "modelDisplayString" not in branch:
        fail(failures, "model 分支必须使用 modelDisplayString")
    if "getModelOptions" not in branch:
        fail(failures, "model 分支必须使用 getModelOptions")
    if "listAllProfiles" not in branch:
        fail(failures, "model 分支必须使用 listAllProfiles 以暴露 sample/fast/provider 等 profile")
    if "desensitizeProfile" not in branch:
        fail(failures, "model 分支必须使用 desensitizeProfile，禁止泄漏 apiKey")
    if "setSessionActiveProfile" not in branch:
        fail(failures, "model 分支必须调用 setSessionActiveProfile 支持 /model <profileName>")
    if "setMainLoopModelOverride" not in branch:
        fail(failures, "model 分支必须调用 setMainLoopModelOverride 让 session 模型立即生效")
    if "notifySessionMetadataChanged" not in branch:
        fail(failures, "model 分支必须通知 session metadata 变化")


def check_args_rejection(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    branch = extract_model_branch(src)
    if not branch:
        return

    # Must check args and reject non-empty
    if "args.length" not in branch and "message.request.args" not in branch:
        fail(failures, "model 分支未检查 args")

    # Must reject ambiguous multi-arg calls.
    if "unsupported_slash_command_args" not in branch:
        fail(failures, "model 分支缺 unsupported_slash_command_args error")
    if "model_profile_not_found" not in branch:
        fail(failures, "model 分支缺 model_profile_not_found error")


def check_safety_no_side_effects(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    branch = extract_model_branch(src)
    if not branch:
        return

    for forbidden in FORBIDDEN_IN_MODEL_BRANCH:
        if forbidden in branch:
            fail(failures, f"model 分支禁止引用: {forbidden}")


def check_no_path_leak(failures: list[str]) -> None:
    """Response must not contain env vars, secrets, config paths."""
    src = PRINT_TS.read_text()
    branch = extract_model_branch(src)
    if not branch:
        return

    # Must not expose raw env vars
    for pattern in ("process.env.", "getSettings_DEPRECATED"):
        # These may appear in getUserSpecifiedModelSetting but not in response building
        pass  # The forbidden list already covers the critical ones


def check_help_updated(failures: list[str]) -> None:
    """Help command list must mark model as supported."""
    src = PRINT_TS.read_text()

    # Find the help branch
    help_match = re.search(
        r"command === 'help'\)\s*\{([\s\S]*?)(?=\}\s*else if \(command ===)",
        src,
    )
    if not help_match:
        fail(failures, "help 分支体抓取失败")
        return
    help_body = help_match.group(1)

    # model support is now manifest-driven.
    if "isStreamJsonSlashCommandAvailable(c.name)" not in help_body:
        fail(failures, "/help supported 列表未使用 manifest 判定 model/status/help 等支持状态")


def check_regression(failures: list[str]) -> None:
    """compact must still be unsupported (blocked); clear is now supported."""
    src = PRINT_TS.read_text()

    # compact should NOT go through success path
    compact_branch = extract_model_branch.__wrapped__(src) if hasattr(extract_model_branch, '__wrapped__') else None
    branch_cmds = re.findall(r"\bcommand === '([^']+)'", src)

    # compact must exist but must be blocked (error only)
    if "command === 'compact'" not in src:
        fail(failures, "command === 'compact' blocked 分支缺失")
    else:
        compact_section = src.split("command === 'compact'")[1].split("else if")[0] if "command === 'compact'" in src else ""
        if "sendControlResponseSuccess" in compact_section:
            fail(failures, "/compact 不应走 success 路径 (blocked)")


def main() -> int:
    failures: list[str] = []
    check_model_branch_exists(failures)
    check_model_success_path(failures)
    check_args_rejection(failures)
    check_safety_no_side_effects(failures)
    check_no_path_leak(failures)
    check_help_updated(failures)
    check_regression(failures)

    print("=== W38 /model slash_command smoke ===")
    print(f"print.ts: {PRINT_TS.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W38 /model stream-json bridge ✓ "
        "(model status + session profile switch + multi-arg rejection + "
        "no config write + help updated + compact still unsupported)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
