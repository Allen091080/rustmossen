#!/usr/bin/env python3
"""
M2.6 — 权限安全 e2e: 4 个 settings source (user / project / local / policy) 优先级.

按 harness全链路测试.md §C.1 M2.6 契约:
  契约: 同一 permission rule 在不同 settings source 时, 高优先级覆盖低优先级.

  Source 路径 (调研 src/utils/settings/settings.ts L240-307):
    userSettings    : $MOSSEN_CONFIG_DIR/settings.json
    projectSettings : <cwd>/.mossen/settings.json
    localSettings   : <cwd>/.mossen/settings.local.json
    policySettings  : (managed file, 本测试不涉及)

  Source 优先级 (调研 src/utils/settings/constants.ts L186-194 SETTINGS_SOURCES_BY_PRIORITY):
    最低 → 最高: userSettings → projectSettings → localSettings → policySettings
    (注: localSettings 列表第一, projectSettings 第二, 此 array 是按"覆盖优先级"
     从高到低排, 详见 settings.ts L801: userSettings -> projectSettings -> localSettings)

  case_project_overrides_user:
    user  ($MOSSEN_CONFIG_DIR/settings.json):  {"permissions":{"allow":["Bash"]}}
    project (<cwd>/.mossen/settings.json):     {"permissions":{"deny":["Bash"]}}
    prompt: 用 Bash echo
    强契约: project 赢 → Bash 被 deny
      - session log: Bash tool_use 出现, tool_result is_error 或含 deny 字面
      - echo marker 不在 tool result 里 (没真执行)

  case_local_overrides_project:
    project: {"permissions":{"allow":["Bash"]}}
    local   (<cwd>/.mossen/settings.local.json): {"permissions":{"deny":["Bash"]}}
    强契约: local 赢 → Bash 被 deny

  反测信号:
    - 改 src/utils/settings/constants.ts 的 SETTINGS_SOURCES_BY_PRIORITY 顺序
      让 userSettings 优先于 projectSettings → case_project_overrides_user 中
      Bash 真执行 (allow 赢) → 我们 expect deny → fail
    - 或改 src/utils/permissions/permissionSetup.ts:applyPermissions 让 deny 被忽略 → fail

  注: 子进程 cwd 必须设到 fixture 内一个 fake-project 目录 (含 .mossen 子目录),
      不然 projectSettings 会落到 mossensrc repo root, 污染或读到无关 .mossen.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

ECHO_MARKER_PROJECT = "M2_6_PROJECT_BASH_OUTPUT_should_NOT_appear"
ECHO_MARKER_LOCAL = "M2_6_LOCAL_BASH_OUTPUT_should_NOT_appear"


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


def _scan_bash(session_logs, marker):
    """检查 Bash tool_use + 关联 tool_result. 返回 dict."""
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
                    if block.get("type") == "tool_use" and block.get("name") == "Bash":
                        attempted = True
                        if block.get("id"):
                            ids.add(block["id"])
    error = 0
    success = 0
    deny_kw = False
    marker_in_result = False
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
                        result_str = str(block.get("content", ""))
                        if block.get("is_error"):
                            error += 1
                        else:
                            success += 1
                        if any(kw in result_str.lower() for kw in deny_keywords):
                            deny_kw = True
                        if marker in result_str:
                            marker_in_result = True
    return {
        "attempted": attempted,
        "success": success,
        "error": error,
        "deny_kw_in_result": deny_kw,
        "marker_in_result": marker_in_result,
    }


def case_project_overrides_user() -> dict:
    """user 写 allow Bash, project 写 deny Bash → project 赢 (Bash 被 deny)."""
    ctx = make_fixture("M2.6_proj_over_user")

    # user-level settings: $MOSSEN_CONFIG_DIR/settings.json
    user_settings_path = ctx.mossen_config_home / "settings.json"
    user_settings_path.write_text(json.dumps({
        "permissions": {"allow": ["Bash"]}
    }, indent=2))

    # 子进程 cwd: fake project root (含 .mossen)
    fake_proj = ctx.root_dir / "fake_project"
    (fake_proj / ".mossen").mkdir(parents=True, exist_ok=True)
    project_settings_path = fake_proj / ".mossen" / "settings.json"
    project_settings_path.write_text(json.dumps({
        "permissions": {"deny": ["Bash"]}
    }, indent=2))

    prompt = f"请用 Bash 工具执行 echo {ECHO_MARKER_PROJECT}, 直接打印"

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p"],
        input=prompt,
        env=_make_env(ctx),
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(fake_proj),  # 关键: cwd 在 fake_project, 这样 projectSettings 才解析到这里
    )
    write_command_log(ctx,
                      ["mossen", "-p", "(cwd=fake_project, user.allow=Bash, project.deny=Bash)"],
                      proc.stdout, proc.stderr, proc.returncode)

    session_logs = _find_session_logs(ctx.home_dir)
    ev = _scan_bash(session_logs, ECHO_MARKER_PROJECT)

    return {
        "name": "project_settings_override_user_settings",
        "expected_winner": "project (deny)",
        "ok": (
            proc.returncode == 0
            and ev["attempted"]                  # Bash 真被尝试
            and (ev["error"] >= 1 or ev["deny_kw_in_result"])  # 被 deny
            and ev["success"] == 0               # 没成功执行
            and not ev["marker_in_result"]       # echo marker 没出现在 result
        ),
        "exit_code": proc.returncode,
        "bash_attempted": ev["attempted"],
        "bash_success": ev["success"],
        "bash_error": ev["error"],
        "deny_kw_in_result": ev["deny_kw_in_result"],
        "marker_in_result": ev["marker_in_result"],
        "user_settings_path": str(user_settings_path),
        "project_settings_path": str(project_settings_path),
        "stdout_excerpt": proc.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def case_local_overrides_project() -> dict:
    """project 写 allow Bash, local 写 deny Bash → local 赢 (Bash 被 deny)."""
    ctx = make_fixture("M2.6_local_over_proj")

    fake_proj = ctx.root_dir / "fake_project"
    (fake_proj / ".mossen").mkdir(parents=True, exist_ok=True)

    project_settings_path = fake_proj / ".mossen" / "settings.json"
    project_settings_path.write_text(json.dumps({
        "permissions": {"allow": ["Bash"]}
    }, indent=2))

    local_settings_path = fake_proj / ".mossen" / "settings.local.json"
    local_settings_path.write_text(json.dumps({
        "permissions": {"deny": ["Bash"]}
    }, indent=2))

    prompt = f"请用 Bash 工具执行 echo {ECHO_MARKER_LOCAL}, 直接打印"

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p"],
        input=prompt,
        env=_make_env(ctx),
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(fake_proj),
    )
    write_command_log(ctx,
                      ["mossen", "-p", "(cwd=fake_project, project.allow=Bash, local.deny=Bash)"],
                      proc.stdout, proc.stderr, proc.returncode)

    session_logs = _find_session_logs(ctx.home_dir)
    ev = _scan_bash(session_logs, ECHO_MARKER_LOCAL)

    return {
        "name": "local_settings_override_project_settings",
        "expected_winner": "local (deny)",
        "ok": (
            proc.returncode == 0
            and ev["attempted"]
            and (ev["error"] >= 1 or ev["deny_kw_in_result"])
            and ev["success"] == 0
            and not ev["marker_in_result"]
        ),
        "exit_code": proc.returncode,
        "bash_attempted": ev["attempted"],
        "bash_success": ev["success"],
        "bash_error": ev["error"],
        "deny_kw_in_result": ev["deny_kw_in_result"],
        "marker_in_result": ev["marker_in_result"],
        "project_settings_path": str(project_settings_path),
        "local_settings_path": str(local_settings_path),
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
    res_pu = _retry(case_project_overrides_user)
    res_lp = _retry(case_local_overrides_project)

    ctx_for_assertions = res_pu["_ctx"]
    results = []
    for r in (res_pu, res_lp):
        r.pop("_ctx", None)
        results.append(r)

    write_assertions(
        ctx_for_assertions,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {"name": r["name"], "expected": True,
             "actual": r.get("ok"), "passed": r.get("ok"),
             "evidence": f"winner={r.get('expected_winner')} "
                         f"exit={r.get('exit_code')} "
                         f"bash_success={r.get('bash_success')} "
                         f"bash_error={r.get('bash_error')} "
                         f"marker_in_result={r.get('marker_in_result')}"}
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "design_note": (
            "M2.6 settings source 优先级 (低 → 高): "
            "user → project → local. project 覆盖 user; local 覆盖 project."
        ),
        "source_paths": {
            "userSettings": "$MOSSEN_CONFIG_DIR/settings.json",
            "projectSettings": "<cwd>/.mossen/settings.json",
            "localSettings": "<cwd>/.mossen/settings.local.json",
        },
        "settings_fields_used": ["permissions.allow", "permissions.deny"],
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
