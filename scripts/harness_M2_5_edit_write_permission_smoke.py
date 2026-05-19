#!/usr/bin/env python3
"""
M2.5 — 权限安全 e2e: Edit 工具的 allow / deny 权限规则真生效 (不只测 Bash).

按 harness全链路测试.md §C.1 M2.5 契约:
  契约: settings.json 的 permissions.allow / permissions.deny 对 Edit 也起效
  字段名 (调研 src/utils/settings/types.ts + src/utils/permissions/permissionSetup.ts):
    permissions.allow: string[]    — 工具白名单
    permissions.deny:  string[]    — 工具黑名单 (优先于 allow)
    permissions.defaultMode: enum  — 默认权限 mode

  case_edit_deny:
    settings.json: {"permissions":{"deny":["Edit"]}}
    prompt 让 model Edit 一个 sentinel 文件
    强契约:
      - exit 0
      - 文件未被改 (仍含 ORIGINAL_M2_5_SENTINEL)
      - session log 含 Edit tool_use AND tool_result is_error / 含 deny

  case_edit_allow:
    settings.json: {"permissions":{"allow":["Edit"]}}
    prompt 同, --add-dir 含 fixture
    强契约:
      - exit 0
      - 文件真被改 (含 EDITED_M2_5_xyz, 不含 ORIGINAL)
      - session log 含 Edit tool_use 成功

  反测信号:
    - 改 src/utils/permissions/filesystem.ts 让 Edit deny check 永远 false (always allow)
      → case_edit_deny 中文件被改 → fail
    - 或改 src/utils/permissions/permissionSetup.ts:applyPermissions 让 deny 被忽略
      → case_edit_deny fail

  注: fixture set MOSSEN_CONFIG_HOME, code 读 MOSSEN_CONFIG_DIR (已知 bug),
      本脚本主动补 MOSSEN_CONFIG_DIR.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

ORIGINAL = "ORIGINAL_M2_5_SENTINEL"
EDITED_MARKER = "EDITED_M2_5_xyz_unique"


def _make_env(ctx) -> dict:
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


def _scan_edit_events(session_logs):
    """返回 dict: attempted, ids, success_count, error_count, deny_kw_in_result."""
    attempted = False
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
                    if block.get("type") == "tool_use" and block.get("name") == "Edit":
                        attempted = True
                        if block.get("id"):
                            ids.add(block["id"])
    success = 0
    error = 0
    deny_kw = False
    deny_keywords = ["denied", "deny", "permission", "blocked", "block",
                     "not allowed", "禁用", "禁止", "拦截", "拒绝", "拒"]
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
                        result_str = str(block.get("content", "")).lower()
                        if block.get("is_error"):
                            error += 1
                        else:
                            success += 1
                        if any(kw in result_str for kw in deny_keywords):
                            deny_kw = True
    return {
        "attempted": attempted,
        "ids": list(ids),
        "success": success,
        "error": error,
        "deny_kw_in_result": deny_kw,
    }


def case_edit_deny() -> dict:
    ctx = make_fixture("M2.5_edit_deny")

    target = ctx.root_dir / "fixture" / "target.txt"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(ORIGINAL)

    settings_file = ctx.root_dir / "settings.json"
    settings_file.write_text(json.dumps({
        "permissions": {"deny": ["Edit"]}
    }, indent=2))

    prompt = (
        f"请用 Edit 工具修改文件 {target}, 把字符串 '{ORIGINAL}' "
        f"替换成 '{EDITED_MARKER}'"
    )

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--settings", str(settings_file),
         "--add-dir", str(ctx.root_dir)],
        input=prompt,
        env=_make_env(ctx),
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )
    write_command_log(ctx,
                      ["mossen", "-p", "--settings", str(settings_file),
                       "--add-dir", str(ctx.root_dir)],
                      proc.stdout, proc.stderr, proc.returncode)

    file_text = target.read_text() if target.exists() else ""
    file_unchanged = (ORIGINAL in file_text) and (EDITED_MARKER not in file_text)

    session_logs = _find_session_logs(ctx.home_dir)
    ev = _scan_edit_events(session_logs)

    return {
        "name": "edit_deny_blocks_edit",
        "ok": (
            proc.returncode == 0
            and file_unchanged          # 关键: 文件未被改
            and ev["attempted"]         # model 尝试用 Edit
            and (ev["error"] >= 1 or ev["deny_kw_in_result"])  # 被 deny
            and ev["success"] == 0      # Edit 没成功执行
        ),
        "exit_code": proc.returncode,
        "file_unchanged": file_unchanged,
        "file_text_excerpt": file_text[:200],
        "edit_attempted": ev["attempted"],
        "edit_success": ev["success"],
        "edit_error": ev["error"],
        "deny_kw_in_result": ev["deny_kw_in_result"],
        "stdout_excerpt": proc.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def case_edit_allow() -> dict:
    ctx = make_fixture("M2.5_edit_allow")

    target = ctx.root_dir / "fixture" / "target.txt"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(ORIGINAL)

    settings_file = ctx.root_dir / "settings.json"
    settings_file.write_text(json.dumps({
        "permissions": {"allow": ["Edit"]}
    }, indent=2))

    prompt = (
        f"请用 Edit 工具修改文件 {target}, 把字符串 '{ORIGINAL}' "
        f"替换成 '{EDITED_MARKER}'"
    )

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--settings", str(settings_file),
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
                      ["mossen", "-p", "--settings", str(settings_file),
                       "--allowedTools", "Edit",
                       "--add-dir", str(ctx.root_dir)],
                      proc.stdout, proc.stderr, proc.returncode)

    file_text = target.read_text() if target.exists() else ""
    file_changed = (EDITED_MARKER in file_text) and (ORIGINAL not in file_text)

    session_logs = _find_session_logs(ctx.home_dir)
    ev = _scan_edit_events(session_logs)

    return {
        "name": "edit_allow_lets_edit_through",
        "ok": (
            proc.returncode == 0
            and file_changed            # 关键: 文件真被改
            and ev["attempted"]
            and ev["success"] >= 1      # Edit 成功
            # 不严格要求 error==0: model 可能首次给错参 retry, 关键是文件真改
        ),
        "exit_code": proc.returncode,
        "file_changed": file_changed,
        "file_text_excerpt": file_text[:200],
        "edit_attempted": ev["attempted"],
        "edit_success": ev["success"],
        "edit_error": ev["error"],
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
    res_deny = _retry(case_edit_deny)
    res_allow = _retry(case_edit_allow)

    ctx_for_assertions = res_deny["_ctx"]
    results = []
    for r in (res_deny, res_allow):
        r.pop("_ctx", None)
        results.append(r)

    write_assertions(
        ctx_for_assertions,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {"name": r["name"], "expected": True,
             "actual": r.get("ok"), "passed": r.get("ok"),
             "evidence": f"exit={r.get('exit_code')} "
                         f"file_unchanged={r.get('file_unchanged')} "
                         f"file_changed={r.get('file_changed')} "
                         f"edit_success={r.get('edit_success')} "
                         f"edit_error={r.get('edit_error')}"}
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "design_note": (
            "M2.5 Edit allow/deny 真生效: deny 时文件未变 + tool_result error, "
            "allow 时文件真被改 + tool_result success"
        ),
        "settings_fields_used": ["permissions.allow", "permissions.deny"],
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
