#!/usr/bin/env python3
"""
Harness fixture helper —— 让所有 e2e smoke 都能用独立 HOME / MOSSEN_CONFIG_HOME / XDG_CONFIG_HOME / artifacts dir。

按 harness全链路测试.md §1.1.2 / §1.1.3 强制要求：
  - 每个测试独立 fixture root: /tmp/mossen-harness/<test-id>/
  - 必须显式覆盖 HOME / MOSSEN_CONFIG_HOME / XDG_CONFIG_HOME
  - MOSSEN_HARNESS=1 标识 harness 环境
  - 不得读写真实 ~/.mossen 或 ~/Documents/aiproject/*
  - 每个测试必须有 artifacts/ 目录含 8 个证据文件

使用例:
    from harness_fixture import make_fixture, write_assertions

    ctx = make_fixture("M0.2")
    # ctx.env, ctx.home_dir, ctx.artifacts_dir 等都已就绪
    # 子进程用 env=ctx.env 启动 mossen
    # 写产物到 ctx.artifacts_dir
    write_assertions(ctx, status="passed", assertions=[...], extra_artifacts={...})
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any

HARNESS_ROOT = Path("/tmp/mossen-harness")

# 真实路径黑名单 —— 任何写到这些路径下的行为都视为污染
FORBIDDEN_PATHS = (
    str(Path.home() / ".mossen"),
    str(Path.home() / "Documents" / "aiproject"),
    str(Path.home() / ".config" / "mossen"),
)


@dataclass
class FixtureContext:
    """单个测试的 fixture 上下文。"""

    test_id: str
    root_dir: Path
    home_dir: Path
    mossen_config_home: Path
    xdg_config_home: Path
    artifacts_dir: Path
    env: dict = field(default_factory=dict)

    def __str__(self) -> str:
        return f"FixtureContext(test_id={self.test_id}, root={self.root_dir})"


def make_fixture(test_id: str, fresh: bool = True) -> FixtureContext:
    """
    建立一个测试隔离 fixture。

    参数:
        test_id: 测试 ID，如 "M0.2" / "M1.1"。文件路径里会被规范化为 dir-safe。
        fresh: 默认 True —— 先清空再创建。False 则保留之前的 artifacts。

    返回 FixtureContext, env dict 已就绪（含 HOME/MOSSEN_CONFIG_HOME/XDG/MOSSEN_HARNESS）。
    """
    if not test_id:
        raise ValueError("test_id 不能为空")

    safe_id = test_id.replace("/", "_").replace(" ", "_")
    root_dir = HARNESS_ROOT / safe_id

    if fresh and root_dir.exists():
        shutil.rmtree(root_dir)

    home_dir = root_dir / "home"
    mossen_config_home = home_dir / ".mossen"
    xdg_config_home = root_dir / "xdg"
    artifacts_dir = root_dir / "artifacts"

    for d in (home_dir, mossen_config_home, xdg_config_home, artifacts_dir):
        d.mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env.update({
        "HOME": str(home_dir),
        "MOSSEN_CONFIG_HOME": str(mossen_config_home),
        "XDG_CONFIG_HOME": str(xdg_config_home),
        "MOSSEN_HARNESS": "1",
        "MOSSEN_HARNESS_TEST_ID": test_id,
    })

    return FixtureContext(
        test_id=test_id,
        root_dir=root_dir,
        home_dir=home_dir,
        mossen_config_home=mossen_config_home,
        xdg_config_home=xdg_config_home,
        artifacts_dir=artifacts_dir,
        env=env,
    )


def assert_no_pollution(ctx: FixtureContext) -> dict:
    """
    检查测试运行后是否污染了真实路径。
    返回 {"ok": bool, "violations": [...]} 字典。
    """
    violations = []
    for forbidden in FORBIDDEN_PATHS:
        forbidden_path = Path(forbidden)
        if not forbidden_path.exists():
            continue
        # 检查 forbidden 目录下是否有以本测试 id 命名的痕迹
        for child in forbidden_path.rglob(f"*{ctx.test_id}*"):
            violations.append({
                "forbidden_root": str(forbidden_path),
                "polluted_path": str(child),
            })
    return {
        "ok": len(violations) == 0,
        "violations": violations,
    }


def write_assertions(
    ctx: FixtureContext,
    status: str,
    assertions: list[dict[str, Any]],
    extra_artifacts: dict[str, str] | None = None,
) -> Path:
    """
    在 artifacts/assertions.json 写机器可读的测试结果。
    按 SOP §1.1.3 格式：
        {"test_id", "status", "assertions": [...], "artifacts": {...}}
    """
    if status not in ("passed", "failed", "blocked", "skipped"):
        raise ValueError(f"非法 status: {status}")

    artifacts = {
        "stdout": str(ctx.artifacts_dir / "stdout.txt"),
        "stderr": str(ctx.artifacts_dir / "stderr.txt"),
        "exit_code": str(ctx.artifacts_dir / "exit_code.txt"),
        "session_log": str(ctx.artifacts_dir / "session_log.jsonl"),
        "command": str(ctx.artifacts_dir / "command.txt"),
        "env": str(ctx.artifacts_dir / "env.txt"),
    }
    if extra_artifacts:
        artifacts.update(extra_artifacts)

    payload = {
        "test_id": ctx.test_id,
        "status": status,
        "timestamp": datetime.now().isoformat(),
        "fixture_root": str(ctx.root_dir),
        "assertions": assertions,
        "artifacts": artifacts,
    }
    target = ctx.artifacts_dir / "assertions.json"
    target.write_text(json.dumps(payload, indent=2, ensure_ascii=False))
    return target


def write_command_log(
    ctx: FixtureContext,
    command: list[str],
    stdout: str,
    stderr: str,
    exit_code: int,
) -> None:
    """快速写入 command/env/stdout/stderr/exit_code 5 个证据文件。"""
    (ctx.artifacts_dir / "command.txt").write_text(" ".join(command))
    (ctx.artifacts_dir / "env.txt").write_text(
        "\n".join(f"{k}={v}" for k, v in sorted(ctx.env.items())
                  if k.startswith(("HOME", "MOSSEN_", "XDG_")))
    )
    (ctx.artifacts_dir / "stdout.txt").write_text(stdout)
    (ctx.artifacts_dir / "stderr.txt").write_text(stderr)
    (ctx.artifacts_dir / "exit_code.txt").write_text(str(exit_code))


def assert_workdir_clean() -> dict:
    """
    确认当前 git 工作树干净（用于 mutation 后还原后的检查）。
    return {"ok": bool, "diff": str}
    """
    repo_root = Path(__file__).resolve().parents[1]
    proc = subprocess.run(
        ["git", "diff", "--exit-code"],
        cwd=str(repo_root),
        capture_output=True,
        text=True,
    )
    return {
        "ok": proc.returncode == 0,
        "diff": proc.stdout[:2000] if proc.returncode != 0 else "",
    }


# CLI for quick verification: `python3 scripts/harness_fixture.py M0.2`
if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("usage: harness_fixture.py <test_id>", file=sys.stderr)
        sys.exit(1)
    ctx = make_fixture(sys.argv[1])
    print(json.dumps({
        "test_id": ctx.test_id,
        "root_dir": str(ctx.root_dir),
        "home_dir": str(ctx.home_dir),
        "mossen_config_home": str(ctx.mossen_config_home),
        "xdg_config_home": str(ctx.xdg_config_home),
        "artifacts_dir": str(ctx.artifacts_dir),
        "env_keys_set": sorted(k for k in ctx.env if k in (
            "HOME", "MOSSEN_CONFIG_HOME", "XDG_CONFIG_HOME",
            "MOSSEN_HARNESS", "MOSSEN_HARNESS_TEST_ID"
        )),
    }, indent=2, ensure_ascii=False))
