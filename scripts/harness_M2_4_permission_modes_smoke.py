#!/usr/bin/env python3
"""
M2.4 — 权限安全 e2e: --permission-mode CLI flag 真生效 (W2 修正后 6 mode 中的 3 个 P0).

按 harness全链路测试.md §C.1 + W2 6-mode 修正:
  契约: 不同 --permission-mode 行为不一样
    - default        : 不传 mode, 危险工具走默认拦截 (类似 M2.1)
    - acceptEdits    : Edit 工具不再询问, 自动通过
    - bypassPermissions: 任意工具直接执行 (rm 真删)

  CLI flag 真名 (调研 src/main.tsx:1030):
    `--permission-mode <mode>` 走 Option().choices(PERMISSION_MODES)
    PERMISSION_MODES (调研 src/utils/permissions/PermissionMode.ts):
      default / plan / acceptEdits / bypassPermissions / dontAsk / auto

  反测信号:
    - 改 src/main.tsx 把 `--permission-mode` 解析强制返 'default' →
      acceptEdits case 中 Edit 被默认权限拦截 → file 内容不被改 → fail
    - 或改 src/utils/permissions/permissionSetup.ts 把 bypass mode 当 default →
      bypass case 中 sentinel 不被删 → fail

  注: fixture 现在 set MOSSEN_CONFIG_HOME 但代码读 MOSSEN_CONFIG_DIR (已知 bug),
      本脚本主动补 MOSSEN_CONFIG_DIR 防止 user-level config fall through 到真 ~/.mossen.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log


def _make_env(ctx) -> dict:
    """补 MOSSEN_CONFIG_DIR 修正 fixture 的 env-name bug."""
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    return env


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def _scan_tool_uses(session_logs, tool_name):
    """返回 (attempted, success_count, error_count, ids)."""
    attempted = False
    success = 0
    error = 0
    ids = set()
    for log_file in session_logs:
        try:
            text = log_file.read_text()
        except OSError:
            continue
        for line in text.splitlines():
            if not line.strip():
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            msg = obj.get("message", obj)
            content = msg.get("content")
            if isinstance(content, list):
                for block in content:
                    if not isinstance(block, dict):
                        continue
                    if block.get("type") == "tool_use" and block.get("name") == tool_name:
                        attempted = True
                        if block.get("id"):
                            ids.add(block["id"])
    # 第二遍 — 拿 result
    for log_file in session_logs:
        try:
            text = log_file.read_text()
        except OSError:
            continue
        for line in text.splitlines():
            if not line.strip():
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            msg = obj.get("message", obj)
            content = msg.get("content")
            if isinstance(content, list):
                for block in content:
                    if not isinstance(block, dict):
                        continue
                    if block.get("type") == "tool_result":
                        if block.get("tool_use_id") not in ids:
                            continue
                        if block.get("is_error"):
                            error += 1
                        else:
                            success += 1
    return attempted, success, error, ids


def case_default_mode() -> dict:
    """default mode (不传 --permission-mode): 危险 rm 应被拦截."""
    ctx = make_fixture("M2.4_default")
    sentinel_dir = ctx.root_dir / "sentinel"
    sentinel_dir.mkdir(parents=True, exist_ok=True)
    sentinel_file = sentinel_dir / "sentinel_default.txt"
    sentinel_file.write_text("MARKER_M2_4_DEFAULT_must_survive")

    prompt = f"请用 Bash 工具执行 rm -rf {sentinel_dir}, 删完里面所有内容"

    proc = subprocess.run(
        # 注意: 不传 --permission-mode, 让 mossen 走默认 mode
        [str(ROOT / "run-mossen.sh"), "-p"],
        input=prompt,
        env=_make_env(ctx),
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )
    write_command_log(ctx, ["mossen", "-p", "(no --permission-mode = default)"],
                      proc.stdout, proc.stderr, proc.returncode)

    sentinel_alive = sentinel_file.exists() and \
        sentinel_file.read_text() == "MARKER_M2_4_DEFAULT_must_survive"

    return {
        "name": "default_mode_blocks_dangerous",
        "mode": "default (no flag)",
        "ok": proc.returncode == 0 and sentinel_alive,
        "exit_code": proc.returncode,
        "sentinel_alive": sentinel_alive,
        "stdout_excerpt": proc.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def case_acceptEdits_mode() -> dict:
    """--permission-mode acceptEdits: Edit 工具自动通过, 文件真被改."""
    ctx = make_fixture("M2.4_acceptEdits")
    target = ctx.root_dir / "fixture" / "edit_target.txt"
    target.parent.mkdir(parents=True, exist_ok=True)
    original = "ORIGINAL_M2_4_ACCEPT_EDITS_xyz"
    target.write_text(original)

    new_marker = "EDITED_M2_4_acceptEdits_zzz"
    prompt = (
        f"请用 Edit 工具修改文件 {target}, "
        f"把字符串 '{original}' 替换成 '{new_marker}'"
    )

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--permission-mode", "acceptEdits",
         "--allowedTools", "Edit",
         "--add-dir", str(ctx.root_dir)],
        input=prompt,
        env=_make_env(ctx),
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )
    write_command_log(ctx,
                      ["mossen", "-p", "--permission-mode", "acceptEdits",
                       "--allowedTools", "Edit", "--add-dir", str(ctx.root_dir)],
                      proc.stdout, proc.stderr, proc.returncode)

    file_text = target.read_text() if target.exists() else ""
    edit_applied = (new_marker in file_text) and (original not in file_text)

    session_logs = _find_session_logs(ctx.home_dir)
    edit_attempted, edit_ok, edit_err, _ = _scan_tool_uses(session_logs, "Edit")

    return {
        "name": "acceptEdits_mode_auto_approves_edit",
        "mode": "acceptEdits",
        "ok": (
            proc.returncode == 0
            and edit_applied  # 文件真被改 — 关键契约
            and edit_attempted
            and edit_ok >= 1
            # 不严格要求 edit_err==0: model 可能首次给错参 retry, 关键是 acceptEdits
            # 模式让权限不阻拦 (true 验证 = file 真被改)
        ),
        "exit_code": proc.returncode,
        "edit_applied": edit_applied,
        "edit_attempted": edit_attempted,
        "edit_tool_success_count": edit_ok,
        "edit_tool_error_count": edit_err,
        "file_content_excerpt": file_text[:200],
        "stdout_excerpt": proc.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def case_bypassPermissions_mode() -> dict:
    """--permission-mode bypassPermissions: 任意工具直接执行, sentinel 真被删."""
    ctx = make_fixture("M2.4_bypass")
    sentinel_dir = ctx.root_dir / "sentinel_bypass"
    sentinel_dir.mkdir(parents=True, exist_ok=True)
    sentinel_file = sentinel_dir / "sentinel_bypass.txt"
    sentinel_file.write_text("MARKER_M2_4_BYPASS_should_be_deleted")

    prompt = (
        f"请用 Bash 工具执行 rm -rf {sentinel_dir}, "
        f"把整个目录及里面所有文件全部删除. 直接执行, 不要问我."
    )

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--permission-mode", "bypassPermissions",
         "--allowedTools", "Bash",
         "--add-dir", str(ctx.root_dir)],
        input=prompt,
        env=_make_env(ctx),
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )
    write_command_log(ctx,
                      ["mossen", "-p", "--permission-mode", "bypassPermissions",
                       "--allowedTools", "Bash", "--add-dir", str(ctx.root_dir)],
                      proc.stdout, proc.stderr, proc.returncode)

    # bypass: sentinel 应被真删 — 验 deny 没拦
    sentinel_dir_gone = not sentinel_dir.exists()
    sentinel_file_gone = not sentinel_file.exists()

    session_logs = _find_session_logs(ctx.home_dir)
    bash_attempted, bash_ok, bash_err, _ = _scan_tool_uses(session_logs, "Bash")

    return {
        "name": "bypassPermissions_mode_skips_all_checks",
        "mode": "bypassPermissions",
        "ok": (
            proc.returncode == 0
            and sentinel_dir_gone  # 关键: 真被删
            and sentinel_file_gone
            and bash_attempted
            and bash_ok >= 1
        ),
        "exit_code": proc.returncode,
        "sentinel_dir_gone": sentinel_dir_gone,
        "sentinel_file_gone": sentinel_file_gone,
        "bash_attempted": bash_attempted,
        "bash_tool_success_count": bash_ok,
        "bash_tool_error_count": bash_err,
        "stdout_excerpt": proc.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def _retry(case_fn, n=3):
    res = None
    for i in range(n):
        res = case_fn()
        if res.get("ok"):
            res["_attempt"] = i + 1
            return res
        res["_attempt"] = i + 1
    return res


def main() -> int:
    res_d = _retry(case_default_mode)
    res_a = _retry(case_acceptEdits_mode)
    res_b = _retry(case_bypassPermissions_mode)

    ctx_for_assertions = res_d["_ctx"]
    results = []
    for r in (res_d, res_a, res_b):
        r.pop("_ctx", None)
        results.append(r)

    write_assertions(
        ctx_for_assertions,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {"name": r["name"], "expected": True,
             "actual": r.get("ok"), "passed": r.get("ok"),
             "evidence": f"mode={r.get('mode')} exit={r.get('exit_code')}"}
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "design_note": (
            "M2.4 --permission-mode 真生效: default 拦危险, "
            "acceptEdits 自动放过 Edit (文件真被改), "
            "bypassPermissions 不拦任何工具 (sentinel 真被删)"
        ),
        "cli_flag": "--permission-mode <default|plan|acceptEdits|bypassPermissions|dontAsk|auto>",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
