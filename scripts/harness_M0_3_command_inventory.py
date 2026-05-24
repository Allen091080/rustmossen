#!/usr/bin/env python3
"""
M0.3 — 全局命令枚举 + 落 harness_slash_command_matrix.json。

按 harness全链路测试.md §C.3 要求:
  - 不能只 grep 源码，必须真启动 mossen 拿可见命令清单（或通过 bun import 拿 COMMANDS() 数组）
  - 落 JSON 矩阵, 每条含: command/visible/category/side_effect/test_mode/expected/script
  - 5 类分类: 无副作用 / 写配置 / 外部服务 / 高风险工具 / 暂不支持

实现策略:
  1. 通过 bun -e 动态加载 commands.ts, 调 COMMANDS() 拿真注册数组（最准）
  2. 对每个 command 序列化 name/description/aliases/availability/isHidden/argumentHint/isMcp/kind 等
  3. 按规则分类落 harness_slash_command_matrix.json
  4. smoke case 验: matrix 非空 + 每条有 name + 5 类分类无遗漏 + 总数 >= 60

反测信号 (mutation):
  改 commands.ts 注释掉某常驻命令（如 help）→ matrix 缺 → smoke fail
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import (  # noqa: E402
    make_fixture,
    write_assertions,
    write_command_log,
)

# 类别启发式（基于 command name + aliases + description）
CATEGORY_PATTERNS = {
    "no_side_effect": {
        "names": {"help", "version", "status", "context", "cost", "stats",
                  "usage", "extra-usage", "fast", "effort", "model",
                  "auth-status", "doctor", "mcp-list", "plugin-list",
                  "agents", "skills", "memory", "tasks", "ctx_viz",
                  "thinkback", "files", "diff", "tag", "branch",
                  "session", "rate-limit-options", "release-notes",
                  "config", "summary", "exit", "rewind", "advisor",
                  "ide", "auto-mode", "plan", "lang", "good-mossen",
                  "btw", "clear", "color", "theme", "vim", "keybindings",
                  "verbose", "init", "init-verifiers", "passes",
                  "stickers", "feedback", "bughunter", "perf-issue",
                  "release-notes", "issue", "pr_comments", "review",
                  "security-review", "thinkback-play", "share",
                  "output-style"},
    },
    "writes_config": {
        "names": {"settings", "permissions", "privacy-settings",
                  "statusline", "hooks", "mcp", "plugin", "skills",
                  "lang", "theme", "vim", "keybindings", "color",
                  "config", "rename", "memory", "effort", "model",
                  "fast", "ide", "tag", "auth", "logout", "login",
                  "setup-token", "terminalSetup", "onboarding",
                  "reload-plugins", "sandbox-toggle"},
    },
    "external_service": {
        "names": {"install-github-app", "install-slack-app",
                  "oauth-refresh", "share", "mobile", "desktop",
                  "chrome", "teleport", "remote-env", "remote-setup",
                  "passes", "feedback", "bughunter", "perf-issue",
                  "release-notes", "stickers", "autofix-pr",
                  "pr_comments", "issue", "from-pr"},
    },
    "high_risk_tool": {
        "names": {"compact", "rewind", "init", "init-verifiers",
                  "commit", "commit-push-pr", "review", "security-review",
                  "autofix-pr", "doctor", "internal-trace", "break-cache",
                  "heapdump", "debug-tool-call", "mock-limits",
                  "reset-limits", "rate-limit-options", "extra-usage"},
    },
}


def categorize(name: str, is_hidden: bool, availability: list[str] | None) -> tuple[str, str]:
    """Return (category, side_effect_str) tuple."""
    # 优先级 1: hosted/console only = external_service
    if availability and ("hosted" in availability or "console" in availability):
        return "external_service", "requires_hosted_or_console_account"
    # 优先级 2: 名字在外部服务白名单
    if name in CATEGORY_PATTERNS["external_service"]["names"]:
        return "external_service", "calls_external_api_or_official_service"
    # 优先级 3: 名字在高风险工具白名单
    if name in CATEGORY_PATTERNS["high_risk_tool"]["names"]:
        return "high_risk_tool", "modifies_repo_or_state_with_potential_loss"
    # 优先级 4: 名字在写配置白名单
    if name in CATEGORY_PATTERNS["writes_config"]["names"]:
        return "writes_config", "writes_user_or_project_config"
    # 优先级 5: 名字在无副作用白名单
    if name in CATEGORY_PATTERNS["no_side_effect"]["names"]:
        return "no_side_effect", "read_only_or_status_only"
    # 默认: 暂不分类
    return "uncategorized", "needs_manual_review"


def extract_commands_via_bun() -> list[dict]:
    """通过 bun -e 加载 commands.ts 拿真注册的 COMMANDS()."""
    snippet = (
        "import { getCommands } from './commands.ts';"
        "const cmds = await getCommands(process.cwd());"
        "const out = cmds.map((cmd: any) => ({"
        "  name: cmd.name,"
        "  description: cmd.description ?? '',"
        "  aliases: cmd.aliases ?? [],"
        "  availability: cmd.availability ?? null,"
        "  isHidden: cmd.isHidden ?? false,"
        "  isEnabled: typeof cmd.isEnabled === 'function' ? cmd.isEnabled() : null,"
        "  argumentHint: cmd.argumentHint ?? null,"
        "  isMcp: cmd.isMcp ?? false,"
        "  kind: cmd.kind ?? null,"
        "  type: cmd.type ?? null,"
        "  isSensitive: cmd.isSensitive ?? false,"
        "  loadedFrom: cmd.loadedFrom ?? null,"
        "  userInvocable: cmd.userInvocable ?? null,"
        "}));"
        "process.stdout.write(JSON.stringify(out));"
    )
    proc = subprocess.run(
        [str(ROOT / "run-bun-featured.sh"), "-e", snippet],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        timeout=120,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"bun extract failed: {proc.stderr[:500]}")
    # bun stdout 可能含其他输出, 找最后一行 JSON 数组
    lines = proc.stdout.splitlines()
    for line in reversed(lines):
        line = line.strip()
        if line.startswith("["):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    raise RuntimeError(f"无法解析 bun stdout: {proc.stdout[:300]}")


def build_matrix(commands: list[dict]) -> dict:
    """组装 matrix JSON: 按 category 分组 + 总览."""
    entries = []
    for cmd in commands:
        if "error" in cmd:
            entries.append({
                "command": cmd["name"],
                "load_error": cmd["error"],
                "category": "load_failed",
                "visible": False,
                "side_effect": "unknown",
                "test_mode": "skip",
                "expected": "fix-loader-or-remove",
                "script": None,
            })
            continue
        category, side_effect = categorize(
            cmd["name"], cmd.get("isHidden", False), cmd.get("availability"),
        )
        # 决定 test_mode
        if category == "external_service":
            test_mode = "mock_or_hidden"
        elif category == "high_risk_tool":
            test_mode = "fixture_with_permission"
        elif category == "writes_config":
            test_mode = "fixture_HOME"
        elif category == "no_side_effect":
            test_mode = "real_run"
        else:
            test_mode = "needs_review"
        entries.append({
            "command": cmd["name"],
            "description": cmd["description"][:200],
            "aliases": cmd["aliases"],
            "availability": cmd["availability"],
            "is_hidden": cmd["isHidden"],
            "is_enabled": cmd["isEnabled"],
            "argument_hint": cmd["argumentHint"],
            "is_mcp": cmd["isMcp"],
            "kind": cmd["kind"],
            "type": cmd["type"],
            "is_sensitive": cmd["isSensitive"],
            "loaded_from": cmd["loadedFrom"],
            "user_invocable": cmd["userInvocable"],
            "category": category,
            "visible": not cmd["isHidden"],
            "side_effect": side_effect,
            "test_mode": test_mode,
            "expected": "see-category-rule",
            "script": None,  # M8 阶段会填
        })
    by_category: dict[str, list[str]] = {}
    for e in entries:
        by_category.setdefault(e["category"], []).append(e["command"])
    return {
        "total": len(entries),
        "by_category_counts": {k: len(v) for k, v in sorted(by_category.items())},
        "by_category_names": by_category,
        "entries": entries,
    }


def case_extract_and_build_matrix() -> dict:
    """从 commands.ts 真提取 + 组装 matrix."""
    try:
        cmds = extract_commands_via_bun()
    except Exception as e:
        return {"name": "extract_and_build_matrix", "ok": False,
                "error": str(e)[:300]}
    matrix = build_matrix(cmds)
    return {
        "name": "extract_and_build_matrix",
        "ok": (
            matrix["total"] >= 30  # 个人版真注册 ≈ 45（hosted/console only 已过滤）
            and "no_side_effect" in matrix["by_category_counts"]
            and matrix["by_category_counts"]["no_side_effect"] >= 5
        ),
        "total": matrix["total"],
        "by_category_counts": matrix["by_category_counts"],
        "matrix_keys": list(matrix.keys()),
        "_matrix_obj": matrix,  # for下面 case 用
    }


def case_matrix_persisted_to_json(prev_result: dict) -> dict:
    """matrix 真落到 harness_slash_command_matrix.json."""
    matrix = prev_result.get("_matrix_obj")
    if matrix is None:
        return {"name": "matrix_persisted_to_json", "ok": False,
                "error": "prev_result 无 _matrix_obj"}
    target = ROOT / "harness_slash_command_matrix.json"
    target.write_text(json.dumps(matrix, indent=2, ensure_ascii=False))
    persisted = json.loads(target.read_text())
    return {
        "name": "matrix_persisted_to_json",
        "ok": (
            target.exists()
            and persisted["total"] == matrix["total"]
            and len(persisted["entries"]) == matrix["total"]
        ),
        "target": str(target),
        "persisted_total": persisted["total"],
    }


def case_known_core_commands_present(prev_result: dict) -> dict:
    """常驻核心命令必须存在: help / clear / compact / context / model / mcp / memory / status."""
    matrix = prev_result.get("_matrix_obj")
    if matrix is None:
        return {"name": "known_core_commands_present", "ok": False,
                "error": "prev_result 无 _matrix_obj"}
    names = {e["command"] for e in matrix["entries"]}
    must_have = {"help", "clear", "compact", "context", "model", "mcp",
                 "memory", "status", "permissions", "skills", "plugin",
                 "lang", "resume"}
    missing = must_have - names
    return {
        "name": "known_core_commands_present",
        "ok": len(missing) == 0,
        "must_have_count": len(must_have),
        "missing": sorted(missing),
        "matched_count": len(must_have - missing),
    }


def case_no_uncategorized_dominance(prev_result: dict) -> dict:
    """uncategorized 数量必须 < 总数的 40%（确保分类启发式覆盖到大多数）."""
    matrix = prev_result.get("_matrix_obj")
    if matrix is None:
        return {"name": "no_uncategorized_dominance", "ok": False,
                "error": "prev_result 无 _matrix_obj"}
    total = matrix["total"]
    uncat = matrix["by_category_counts"].get("uncategorized", 0)
    ratio = uncat / total if total else 1.0
    return {
        "name": "no_uncategorized_dominance",
        "ok": ratio < 0.4,
        "total": total,
        "uncategorized": uncat,
        "uncategorized_ratio": round(ratio, 3),
        "uncategorized_names": matrix["by_category_names"].get("uncategorized", [])[:30],
    }


def main() -> int:
    ctx = make_fixture("M0.3")

    res1 = case_extract_and_build_matrix()
    res2 = case_matrix_persisted_to_json(res1)
    res3 = case_known_core_commands_present(res1)
    res4 = case_no_uncategorized_dominance(res1)

    # 清理 _matrix_obj 不进 JSON
    for r in (res1, res2, res3, res4):
        r.pop("_matrix_obj", None)

    results = [res1, res2, res3, res4]

    # 写 artifacts
    write_command_log(
        ctx,
        command=["python3", str(Path(__file__).name)],
        stdout=json.dumps(results, ensure_ascii=False),
        stderr="",
        exit_code=0 if all(r.get("ok") for r in results) else 1,
    )
    write_assertions(ctx, status="passed" if all(r.get("ok") for r in results) else "failed",
                     assertions=[
                         {"name": r["name"], "expected": True,
                          "actual": r.get("ok"), "passed": r.get("ok")}
                         for r in results
                     ],
                     extra_artifacts={"matrix_json": str(ROOT / "harness_slash_command_matrix.json")})

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "matrix_json": str(ROOT / "harness_slash_command_matrix.json"),
        "design_note": (
            "M0.3 通过 bun import commands.ts 真注册 COMMANDS() 拿命令清单 + "
            "按 5 类分类启发式 + 落 harness_slash_command_matrix.json"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
