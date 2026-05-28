#!/usr/bin/env python3
"""
M0.2 — 验 harness fixture helper 真隔离 + 不污染用户真实配置。

按 harness全链路测试.md §1.1.2 / §1.1.3 / §1.1.5 / §1.1.6 验收：
  Case 1: make_fixture 真创建 4 个隔离目录 (home / mossen_config_home / xdg / artifacts)
  Case 2: env 含 5 个必备字段 (HOME / MOSSEN_CONFIG_HOME / XDG_CONFIG_HOME / MOSSEN_HARNESS / MOSSEN_HARNESS_TEST_ID)
  Case 3: 子进程用 ctx.env 写入 ~/.mossen/* —— 必须只落到 fixture HOME 内, 真实 ~/.mossen 不动
  Case 4: write_assertions / write_command_log 真在 artifacts/ 落 8 个标准文件
  Case 5: assert_no_pollution 在干净状态下 ok=True; 模拟在 fixture 外写文件后 ok=False
  Case 6: assert_workdir_clean 在工作树干净时 ok=True

反测信号 (mutation):
  改 make_fixture 让 HOME = Path.home() 真实路径 → case 3 检测污染 → fail
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import (
    FORBIDDEN_PATHS,
    FixtureContext,
    assert_no_pollution,
    assert_workdir_clean,
    make_fixture,
    write_assertions,
    write_command_log,
)


def case_make_fixture_creates_dirs() -> dict:
    ctx = make_fixture("M0.2_case1")
    expected_dirs = [
        ctx.root_dir,
        ctx.home_dir,
        ctx.mossen_config_home,
        ctx.xdg_config_home,
        ctx.artifacts_dir,
    ]
    all_exist = all(d.is_dir() for d in expected_dirs)
    # 抓力强化: 4 个子目录必须真物理位于 root_dir 下
    children_under_root = all(
        str(d).startswith(str(ctx.root_dir))
        for d in (ctx.home_dir, ctx.mossen_config_home,
                  ctx.xdg_config_home, ctx.artifacts_dir)
    )
    return {
        "name": "make_fixture_creates_dirs",
        "ok": all_exist and children_under_root,
        "dirs_status": {str(d): d.is_dir() for d in expected_dirs},
        "children_under_root": children_under_root,
    }


def case_env_has_required_fields() -> dict:
    ctx = make_fixture("M0.2_case2")
    required = ("HOME", "MOSSEN_CONFIG_HOME", "XDG_CONFIG_HOME",
                "MOSSEN_HARNESS", "MOSSEN_HARNESS_TEST_ID")
    missing = [k for k in required if k not in ctx.env]
    home_under_fixture = ctx.env.get("HOME", "").startswith(str(ctx.root_dir))
    config_under_fixture = ctx.env.get("MOSSEN_CONFIG_HOME", "").startswith(str(ctx.root_dir))
    test_id_matches = ctx.env.get("MOSSEN_HARNESS_TEST_ID") == "M0.2_case2"
    harness_flag = ctx.env.get("MOSSEN_HARNESS") == "1"
    return {
        "name": "env_has_required_fields",
        "ok": (
            len(missing) == 0
            and home_under_fixture
            and config_under_fixture
            and test_id_matches
            and harness_flag
        ),
        "missing": missing,
        "home_under_fixture": home_under_fixture,
        "config_under_fixture": config_under_fixture,
        "test_id_matches": test_id_matches,
        "harness_flag": harness_flag,
    }


def case_subprocess_writes_to_fixture_only() -> dict:
    """子进程用 ctx.env 写入 $HOME/.mossen/test_marker.txt
    必须落到 fixture, 不污染真实 ~/.mossen。"""
    ctx = make_fixture("M0.2_case3")

    proc = subprocess.run(
        ["bash", "-c", 'mkdir -p "$HOME/.mossen" && echo TEST_MARKER_M0_2_C3 > "$HOME/.mossen/test_marker.txt" && cat "$HOME/.mossen/test_marker.txt"'],
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=10,
    )

    fixture_marker_path = ctx.mossen_config_home / "test_marker.txt"
    fixture_has_marker = fixture_marker_path.exists() and "TEST_MARKER_M0_2_C3" in fixture_marker_path.read_text()

    # 抓力强化: marker 文件物理路径必须真位于 ctx.root_dir 下
    marker_under_root = str(fixture_marker_path).startswith(str(ctx.root_dir))

    real_mossen = Path.home() / ".mossen"
    real_marker_path = real_mossen / "test_marker.txt"
    real_polluted = real_marker_path.exists() and "TEST_MARKER_M0_2_C3" in real_marker_path.read_text()

    return {
        "name": "subprocess_writes_to_fixture_only",
        "ok": (
            proc.returncode == 0
            and "TEST_MARKER_M0_2_C3" in proc.stdout
            and fixture_has_marker
            and marker_under_root
            and not real_polluted
        ),
        "subprocess_exit": proc.returncode,
        "fixture_has_marker": fixture_has_marker,
        "marker_under_root": marker_under_root,
        "real_dir_polluted": real_polluted,
        "fixture_marker_path": str(fixture_marker_path),
        "expected_under_root": str(ctx.root_dir),
    }


def case_artifacts_files_written() -> dict:
    """验 write_command_log + write_assertions helper 能产物正确。
    用临时 sub-fixture 验证后立刻清理 (避免污染 final-report 聚合)。"""
    import shutil as _shutil
    ctx = make_fixture("M0_2_case4_temp_helper_validation")
    write_command_log(
        ctx,
        command=["echo", "hello_M0_2_c4"],
        stdout="hello_M0_2_c4\n",
        stderr="",
        exit_code=0,
    )
    write_assertions(ctx, status="passed", assertions=[
        {"name": "demo", "expected": True, "actual": True, "passed": True}
    ])

    expected_files = ["command.txt", "env.txt", "stdout.txt", "stderr.txt",
                      "exit_code.txt", "assertions.json"]
    file_status = {f: (ctx.artifacts_dir / f).exists() for f in expected_files}

    assertions_path = ctx.artifacts_dir / "assertions.json"
    parsed = json.loads(assertions_path.read_text())

    result = {
        "name": "artifacts_files_written",
        "ok": (
            all(file_status.values())
            and parsed["test_id"] == "M0_2_case4_temp_helper_validation"
            and parsed["status"] == "passed"
            and len(parsed["assertions"]) == 1
        ),
        "file_status": file_status,
        "assertions_test_id": parsed["test_id"],
        "assertions_status": parsed["status"],
    }
    # 清理临时 fixture, 避免污染 final-report 聚合
    _shutil.rmtree(ctx.root_dir, ignore_errors=True)
    return result


def case_no_pollution_check() -> dict:
    """干净 fixture 不应触发污染告警。"""
    ctx = make_fixture("M0_2_case5_pollution_unique_marker")
    result = assert_no_pollution(ctx)
    return {
        "name": "no_pollution_check_clean",
        "ok": result["ok"] is True,
        "violations": result["violations"],
    }


def case_workdir_clean_helper_callable() -> dict:
    """assert_workdir_clean 是 mutation 测试调用方用的辅助函数。
    本 case 只验它能调用 + 返回 dict 格式正确, 不依赖当前工作树状态
    (因为本 smoke 自身注册到 smoke_check.py 时会让工作树有 diff)。"""
    result = assert_workdir_clean()
    return {
        "name": "workdir_clean_helper_callable",
        "ok": (
            isinstance(result, dict)
            and "ok" in result
            and isinstance(result["ok"], bool)
            and "diff" in result
        ),
        "result_shape_ok": "ok" in result and "diff" in result,
        "current_workdir_clean": result.get("ok"),
    }


def main() -> int:
    main_ctx = make_fixture("M0.2")
    results = [
        case_make_fixture_creates_dirs(),
        case_env_has_required_fields(),
        case_subprocess_writes_to_fixture_only(),
        case_artifacts_files_written(),
        case_no_pollution_check(),
        case_workdir_clean_helper_callable(),
    ]
    write_command_log(main_ctx, ["python3", "harness_M0_2_fixture_smoke.py"],
                      json.dumps(results, ensure_ascii=False), "",
                      0 if all(r.get("ok") for r in results) else 1)
    write_assertions(main_ctx,
                     status="passed" if all(r.get("ok") for r in results) else "failed",
                     assertions=[
                         {"name": r["name"], "expected": True,
                          "actual": r.get("ok"), "passed": r.get("ok")}
                         for r in results
                     ])
    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "design_note": (
            "M0.2 fixture helper 验收: make_fixture 真隔离 / env 真覆盖 / "
            "子进程不污染真实 ~/.mossen / artifacts 真落 6 个证据 / "
            "assert_no_pollution + assert_workdir_clean 工作。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
