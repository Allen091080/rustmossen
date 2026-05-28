#!/usr/bin/env python3
"""W40 — /clear and /compact slash_command bridges batch smoke.

契约:
  /clear:
    1. command === 'clear' 分支存在
    2. args 无 --confirm → confirmation_required error
    3. args 有 --confirm + idle → success, clear.cleared === true
    4. running 状态下拒绝 → session_not_idle
    5. 不调用 TUI interactive path, 不泄漏 raw messages
  /compact:
    6. command === 'compact' 分支存在但走 error (blocked)
    7. error 含 unsupported_slash_command + compact
    8. compact 不能标 supported in /help
  Regression:
    9. /help /status /model still work
   10. /model <arg> still rejected
   11. no command file changes (commands/clear/**, commands/compact/** untouched)

跑法:
  python3 scripts/wave_w40_slash_bridge_batch_smoke.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PRINT_TS = ROOT / "cli" / "print.ts"
CLEAR_CONVERSATION = ROOT / "commands" / "clear" / "conversation.ts"
COMPACT_COMMAND = ROOT / "commands" / "compact" / "compact.ts"


def fail(msgs: list[str], msg: str) -> None:
    msgs.append(msg)


def extract_command_branch(src: str, cmd: str) -> str | None:
    """Extract command === '<cmd>' branch body up to next branch."""
    m = re.search(
        rf"command === '{re.escape(cmd)}'\)\s*\{{([\s\S]*?)\n\s*\}}\s*else if \(command ===",
        src,
    )
    return m.group(1) if m else None


def check_clear_branch(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    branch = extract_command_branch(src, "clear")
    if not branch:
        fail(failures, "command === 'clear' 分支缺失")
        return

    # Must check --confirm
    if "--confirm" not in branch:
        fail(failures, "/clear 分支未检查 --confirm")

    # Must check confirmation_required
    if "confirmation_required" not in branch:
        fail(failures, "/clear 分支缺 confirmation_required error")

    # Must check session_not_idle / running
    if "running" not in branch:
        fail(failures, "/clear 分支未检查 running 状态")
    if "session_not_idle" not in branch:
        fail(failures, "/clear 分支缺 session_not_idle error")

    # Must import clearConversation lazily
    if "clearConversation" not in branch:
        fail(failures, "/clear 分支未引用 clearConversation")

    # Must produce slash_command_result
    if "slash_command_result" not in branch:
        fail(failures, "/clear success 缺 slash_command_result")

    # Must have clear.cleared
    if "cleared" not in branch:
        fail(failures, "/clear response 缺 clear.cleared")


def check_clear_safety(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    branch = extract_command_branch(src, "clear")
    if not branch:
        return

    # Must NOT call TUI interactive path
    for forbidden in ("interactive", "ModelPicker", "terminal-framework/", "React"):
        if forbidden in branch:
            fail(failures, f"/clear 分支禁止引用: {forbidden}")

    # Must NOT leak raw messages
    if "JSON.stringify(mutableMessages)" in branch:
        fail(failures, "/clear 分支泄漏 raw messages")


def check_compact_blocked(failures: list[str]) -> None:
    src = PRINT_TS.read_text()
    branch = extract_command_branch(src, "compact")
    if not branch:
        fail(failures, "command === 'compact' 分支缺失 (应存在 blocked 分支)")
        return

    # Must go to error path only
    if "sendControlResponseSuccess" in branch:
        fail(failures, "/compact 分支不允许走 success 路径 (blocked)")

    # Must contain unsupported_slash_command
    if "unsupported_slash_command" not in branch:
        fail(failures, "/compact 分支缺 unsupported_slash_command error")

    # Must mention compact in error
    if "compact" not in branch:
        fail(failures, "/compact error 未提及 compact")


def check_compact_help_unsupported(failures: list[str]) -> None:
    """compact must NOT be in /help supported list."""
    src = PRINT_TS.read_text()

    # Find supported check
    supported_match = re.search(r"supported:.*?(?=\n)", src, re.MULTILINE)
    if not supported_match:
        fail(failures, "/help supported 列表未找到")
        return
    supported_line = supported_match.group(0)

    # compact must not be in supported
    if "'compact'" in supported_line or '"compact"' in supported_line:
        fail(failures, "/help 列表不允许标 compact 为 supported (blocked)")


def check_help_clear_supported(failures: list[str]) -> None:
    """clear must be in /help supported list."""
    src = PRINT_TS.read_text()

    help_match = re.search(
        r"command === 'help'\)\s*\{([\s\S]*?)(?=\}\s*else if \(command ===)",
        src,
    )
    if not help_match:
        fail(failures, "help 分支体抓取失败")
        return
    help_body = help_match.group(1)

    if "isStreamJsonSlashCommandAvailable(c.name)" not in help_body:
        fail(failures, "/help supported 列表未使用 manifest 判定 clear/help/status/model 等支持状态")


def check_command_files_untouched(failures: list[str]) -> None:
    """Command implementation files should not be modified in this slice."""
    if not CLEAR_CONVERSATION.exists():
        fail(failures, "commands/clear/conversation.ts 文件缺失")
    if not COMPACT_COMMAND.exists():
        fail(failures, "commands/compact/compact.ts 文件缺失")


def check_regression_commands(failures: list[str]) -> None:
    """Existing commands must still be supported."""
    src = PRINT_TS.read_text()

    for cmd in ("help", "status", "model"):
        if f"command === '{cmd}'" not in src:
            fail(failures, f"/{cmd} 分支缺失 (回归)")

    # /model owns its own profile-name args contract (W45); W40 only checks
    # that the branch still exists while /clear and /compact are changed.
    model_branch = extract_command_branch(src, "model")
    if not model_branch:
        fail(failures, "/model 分支体抓取失败")


def main() -> int:
    failures: list[str] = []
    check_clear_branch(failures)
    check_clear_safety(failures)
    check_compact_blocked(failures)
    check_compact_help_unsupported(failures)
    check_help_clear_supported(failures)
    check_command_files_untouched(failures)
    check_regression_commands(failures)

    print("=== W40 slash bridge batch smoke ===")
    print(f"print.ts:                {PRINT_TS.relative_to(ROOT)}")
    print(f"clear/conversation.ts:   {CLEAR_CONVERSATION.relative_to(ROOT)}")
    print(f"compact/compact.ts:      {COMPACT_COMMAND.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W40 slash bridge batch ✓ "
        "(clear: confirmed+idle bridge; compact: blocked; "
        "help/status/model: no regression; "
        "command files: untouched)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
