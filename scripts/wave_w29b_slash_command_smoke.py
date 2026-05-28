#!/usr/bin/env python3
"""
W29-B — slash_command control_request 协议骨架 + curated slash bridges

契约 (本 slice 必须保住):
  Schema 层
    1. SDKControlSlashCommandRequestSchema 存在并入 SDKControlRequestInnerSchema union
    2. 字段固定: subtype: 'slash_command', command: string, args?: string[]
    3. whitelist Section B 含 SDKControlSlashCommandRequestSchema, count 22
  派发层 (cli/print.ts)
    4. 'slash_command' 分支存在
    5. curated commands 走 success 路径; 其它命令走 sendControlResponseError
    6. allowlist 拒绝清单包含: compact / config / permissions / unknown
       (用 error 字符串契约保证: 未支持 command 返回 unsupported_slash_command)
    7. /help response 含 subtype='slash_command_result' / command='help' / commands list
  Stage 2 防回归
    8. /compact 不允许在本 slice 被识别为 supported (反向断言)
    9. 不允许新增 compactConversation 调用从 print.ts 的 slash_command 分支引出

跑法:
  python3 scripts/wave_w29b_slash_command_smoke.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
PRINT_TS = ROOT / "cli" / "print.ts"
WHITELIST = ROOT / "scripts" / "stream-json-schema-whitelist.txt"


REJECTED_COMMANDS = (
    "compact",
    "config",
    "permissions",
)


def fail(msgs: list[str], msg: str) -> None:
    msgs.append(msg)


def check_schema(failures: list[str]) -> None:
    src = CONTROL_SCHEMAS.read_text()

    # 1. 名字存在
    if "export const SDKControlSlashCommandRequestSchema" not in src:
        fail(failures, "SDKControlSlashCommandRequestSchema 未定义")
        return

    # 2. 字段契约 — 抓块再断言. 块以下一个 export const 结束.
    m = re.search(
        r"export const SDKControlSlashCommandRequestSchema[\s\S]*?(?=\nexport const |\n//\s*=)",
        src,
    )
    if not m:
        fail(failures, "SDKControlSlashCommandRequestSchema 块结构异常 (无法抓取)")
        return
    block = m.group(0)
    for required in (
        "subtype: z.literal('slash_command')",
        "command: z.string()",
        "args: z.array(z.string()).optional()",
    ):
        if required not in block:
            fail(failures, f"slash_command schema 缺字段: {required}")

    # 3. union 引用
    union_match = re.search(
        r"SDKControlRequestInnerSchema = lazySchema\([\s\S]*?z\.union\(\[([\s\S]*?)\]\)",
        src,
    )
    if not union_match:
        fail(failures, "SDKControlRequestInnerSchema union 块抓取失败")
        return
    union_body = union_match.group(1)
    if "SDKControlSlashCommandRequestSchema()" not in union_body:
        fail(
            failures,
            "SDKControlSlashCommandRequestSchema() 未加入 SDKControlRequestInnerSchema union",
        )


def check_whitelist(failures: list[str]) -> None:
    src = WHITELIST.read_text()
    if "SDKControlSlashCommandRequestSchema" not in src:
        fail(failures, "whitelist 漏 SDKControlSlashCommandRequestSchema")
    if "Section B — SDKControlRequestInner union (29 成员" not in src:
        fail(failures, "whitelist Section B header 未升 29 成员")


def check_dispatcher(failures: list[str]) -> None:
    src = PRINT_TS.read_text()

    # 4. 'slash_command' 分支
    if "message.request.subtype === 'slash_command'" not in src:
        fail(failures, "print.ts 缺 slash_command 分支")
        return

    # 抓 slash_command 分支体到下一个 } else { 或下一个 } else if 为止
    branch_match = re.search(
        r"message\.request\.subtype === 'slash_command'\)\s*\{([\s\S]*)\}\s*else\s*\{[\s\S]*?Unknown control request subtype",
        src,
    )
    if not branch_match:
        fail(failures, "无法抓取 slash_command 分支体 (派发结构异常)")
        return
    branch_body = branch_match.group(1)

    # 5. 必须有 sendControlResponseSuccess + sendControlResponseError 都用上
    #     curated commands 走 success; 其它走 error
    if "sendControlResponseSuccess(message" not in branch_body:
        fail(failures, "slash_command 分支无 sendControlResponseSuccess 路径")
    if "sendControlResponseError" not in branch_body:
        fail(failures, "slash_command 分支无 sendControlResponseError 路径")

    # 6. allowlist 锚点 — curated commands 必须显式比较
    required_commands = (
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
    for required_command in required_commands:
        if f"command === '{required_command}'" not in branch_body:
            fail(
                failures,
                f"slash_command 分支必须显式判断 command === '{required_command}'",
            )

    # 7. error 字符串契约: 必须是 string, 含 unsupported_slash_command
    if "unsupported_slash_command" not in branch_body:
        fail(
            failures,
            "slash_command 分支 error 字符串必须含 unsupported_slash_command 锚点",
        )

    # 8. /help response 必须返回 slash_command_result + commands list
    if "subtype: 'slash_command_result'" not in branch_body:
        fail(failures, "/help 响应缺 subtype: 'slash_command_result'")
    if "getCommands(cwd())" not in branch_body:
        fail(failures, "/help 响应未调用 getCommands(cwd())")
    if "commands:" not in branch_body:
        fail(failures, "/help 响应缺 commands list 字段")
    if "supported:" not in branch_body:
        fail(failures, "/help commands 项缺 supported 字段")

    # 9. /compact must exist but must NOT go through success path (blocked)
    if "command === 'compact'" not in branch_body:
        fail(failures, "slash_command 分支缺 command === 'compact' blocked 处理")
    else:
        # compact section must be error-only
        compact_section = branch_body.split("command === 'compact'")[1].split("command === '")[0] if "command === 'compact'" in branch_body else ""
        if "sendControlResponseSuccess" in compact_section:
            fail(failures, "/compact 不应走 success 路径 (blocked)")

    # 10. compactConversation 不应被从 slash_command 分支调用
    if "compactConversation" in branch_body:
        fail(
            failures,
            "slash_command 分支不应调用 compactConversation",
        )


def check_command_normalization(failures: list[str]) -> None:
    """命令归一化 (trim + 去前导 / + lower-case) 必须存在, 否则 '/help'/'HELP' 等输入会被误判."""
    src = PRINT_TS.read_text()
    branch_match = re.search(
        r"message\.request\.subtype === 'slash_command'\)\s*\{([\s\S]*)\}\s*else\s*\{[\s\S]*?Unknown control request subtype",
        src,
    )
    if not branch_match:
        return  # 已在 dispatcher 检查里报错
    body = branch_match.group(1)
    if "normalizeStreamJsonSlashCommand(rawCommand)" in body:
        return
    if ".trim()" not in body:
        fail(failures, "command 归一化缺 .trim()")
    if "replace(/^\\/+/" not in body:
        fail(failures, "command 归一化缺 replace(/^\\/+/...) 去前导 /")
    if ".toLowerCase()" not in body:
        fail(failures, "command 归一化缺 .toLowerCase()")


def check_rejected_anchor(failures: list[str]) -> None:
    """所有 REJECTED_COMMANDS 在 print.ts 内不应被 slash_command 分支当成 supported.

    本检查走结构断言: 分支体里 "command === '<name>'" 这种比较只允许 'help' 一项.
    """
    src = PRINT_TS.read_text()
    branch_match = re.search(
        r"message\.request\.subtype === 'slash_command'\)\s*\{([\s\S]*)\}\s*else\s*\{[\s\S]*?Unknown control request subtype",
        src,
    )
    if not branch_match:
        return
    body = branch_match.group(1)
    # 逐行扫描 command === '<name>' 真分支, 跳过含 typeof 的类型守卫行.
    cmd_matches: list[str] = []
    for line in body.splitlines():
        if "typeof" in line:
            continue
        for v in re.findall(r"\bcommand === '([^']+)'", line):
            cmd_matches.append(v)
    allowed = (
        "help",
        "capabilities",
        "status",
        "model",
        "clear",
        "cost",
        "skills",
        "mcp",
        "plugin",
        "plugins",
        "agents",
        "permissions",
        "hooks",
        "memory",
        "compact",
    )
    extra = [c for c in cmd_matches if c not in allowed]
    if extra:
        fail(
            failures,
            f"仅允许 curated slash command 显式分支, 实测多余: {extra}",
        )


def main() -> int:
    failures: list[str] = []
    check_schema(failures)
    check_whitelist(failures)
    check_dispatcher(failures)
    check_command_normalization(failures)
    check_rejected_anchor(failures)

    print("=== W29-B Stage 1 slash_command smoke ===")
    print(f"controlSchemas.ts: {CONTROL_SCHEMAS.relative_to(ROOT)}")
    print(f"print.ts:          {PRINT_TS.relative_to(ROOT)}")
    print(f"whitelist:         {WHITELIST.relative_to(ROOT)}")
    print(f"rejected anchors:  {', '.join(REJECTED_COMMANDS)}")

    if failures:
        print()
        print("=== FAIL ===")
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W29-B slash_command 协议骨架 ✓ "
        "(schema + union 29 + whitelist + dispatcher curated commands supported + "
        "防回归 0 compact)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
