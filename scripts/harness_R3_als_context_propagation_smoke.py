#!/usr/bin/env python3
"""
R3 — ALS context 传播 + tool_use/tool_result id 关联的安全网测试.

按 OpenTelemetry删除计划.md §0.4.4 (C agent 设计) + §3 Layer 1.

守护契约:
  prompt = "请用 Bash 执行 sleep 0.5 && echo R3_TEST_MARKER"
  断言:
    1. exit_code == 0
    2. R3_TEST_MARKER 出现在 tool_result.content 里 (工具真执行)
    3. session jsonl 含 tool_use(name=Bash, id=X) + tool_result(tool_use_id=X)
    4. tool_use.id 集合 ⊆ tool_result.tool_use_id 集合

反测信号:
  - 删 tool_use.id 生成 → undefined → R3 fail
  - 删 tool_result.tool_use_id → 浮游 → R3 fail
  - Bash 改 no-op → marker 不出现 → R3 fail
  - ALS 拆 OTel 后 async tool 调用断 (stub span 不满足 interface) → R3 fail

与 M1.2 区别:
  M1.2: marker 在 stdout + tool_use.command 含 marker (粗粒度)
  R3:   tool_use.id ↔ tool_result.tool_use_id 关联 (细粒度, ALS 守护)
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

MARKER = "R3_TEST_MARKER_xyz"


def _make_env(ctx) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    return env


def _find_session_logs(home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in home.glob(pattern):
            if p.is_file() and p not in found:
                found.append(p)
    return found


def _scan_tool_pairing(session_logs: list[Path], expected_marker: str) -> dict:
    """收集所有 tool_use(name=Bash) ↔ tool_result 关联状态."""
    bash_use_ids = set()
    bash_use_inputs = {}  # id -> input.command
    tool_result_by_id = {}  # tool_use_id -> result content str
    marker_in_some_result = False

    for log in session_logs:
        try:
            text = log.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for line in text.splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                ev = json.loads(line)
            except json.JSONDecodeError:
                continue
            msg = ev.get("message", ev) if isinstance(ev, dict) else {}
            content = msg.get("content") if isinstance(msg, dict) else None
            if not isinstance(content, list):
                continue
            for block in content:
                if not isinstance(block, dict):
                    continue
                btype = block.get("type")
                if btype == "tool_use" and block.get("name") == "Bash":
                    bid = block.get("id")
                    if bid:
                        bash_use_ids.add(bid)
                        inp = block.get("input")
                        if isinstance(inp, dict):
                            bash_use_inputs[bid] = str(inp.get("command", ""))
                elif btype == "tool_result":
                    tid = block.get("tool_use_id")
                    if tid:
                        result_str = str(block.get("content", ""))
                        tool_result_by_id[tid] = result_str
                        if expected_marker in result_str:
                            marker_in_some_result = True

    paired_ids = bash_use_ids & set(tool_result_by_id.keys())
    unpaired_uses = bash_use_ids - set(tool_result_by_id.keys())

    return {
        "bash_use_ids": sorted(bash_use_ids),
        "tool_result_ids": sorted(tool_result_by_id.keys()),
        "paired_ids": sorted(paired_ids),
        "unpaired_uses": sorted(unpaired_uses),
        "marker_in_some_result": marker_in_some_result,
        "bash_use_inputs": bash_use_inputs,
        "tool_result_count": len(tool_result_by_id),
    }


def case_als_context_propagation() -> dict:
    ctx = make_fixture("R3_als_ctx")
    env = _make_env(ctx)

    fake_proj = ctx.root_dir / "fake_project"
    fake_proj.mkdir(parents=True, exist_ok=True)

    prompt = f"请用 Bash 工具执行: sleep 0.5 && echo {MARKER}"

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p", "--allowedTools", "Bash"],
        input=prompt,
        env=env,
        capture_output=True,
        text=True,
        timeout=300,  # sleep 0.5 + LLM 余裕
        cwd=str(fake_proj),
    )
    write_command_log(
        ctx,
        ["mossen", "-p", "--allowedTools", "Bash"],
        proc.stdout,
        proc.stderr,
        proc.returncode,
    )

    session_logs = _find_session_logs(ctx.home_dir)
    pairing = _scan_tool_pairing(session_logs, MARKER)

    bash_attempted = len(pairing["bash_use_ids"]) > 0
    pairing_complete = len(pairing["unpaired_uses"]) == 0
    pairing_nonempty = len(pairing["paired_ids"]) > 0

    ok = (
        proc.returncode == 0
        and bash_attempted
        and pairing_nonempty           # 至少 1 对配上
        and pairing_complete           # 所有 tool_use 都有 result
        and pairing["marker_in_some_result"]  # 真执行
    )

    return {
        "name": "als_context_propagation_tool_pairing",
        "ok": ok,
        "exit_code": proc.returncode,
        "bash_attempted": bash_attempted,
        "bash_use_count": len(pairing["bash_use_ids"]),
        "tool_result_count": pairing["tool_result_count"],
        "paired_count": len(pairing["paired_ids"]),
        "unpaired_uses": pairing["unpaired_uses"],
        "marker_in_result": pairing["marker_in_some_result"],
        "stdout_excerpt": proc.stdout[:300],
        "stderr_excerpt": proc.stderr[:300],
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
    res = _retry(case_als_context_propagation)
    ctx = res.pop("_ctx")

    write_assertions(
        ctx,
        status="passed" if res.get("ok") else "failed",
        assertions=[{
            "name": res["name"],
            "expected": True,
            "actual": res.get("ok"),
            "passed": res.get("ok"),
            "evidence": (
                f"exit={res.get('exit_code')} "
                f"bash_use={res.get('bash_use_count')} "
                f"paired={res.get('paired_count')} "
                f"marker_in_result={res.get('marker_in_result')}"
            ),
        }],
    )

    summary = {
        "results": [res],
        "passed": 1 if res.get("ok") else 0,
        "total": 1,
        "design_note": (
            "R3: tool_use(name=Bash).id ⇄ tool_result.tool_use_id 关联 "
            "+ marker 真出现在 tool_result.content (ALS 业务逻辑守护)"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if res.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
