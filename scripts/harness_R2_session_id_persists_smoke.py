#!/usr/bin/env python3
"""
R2 — session_id 持久化与文件名匹配的安全网测试.

按 OpenTelemetry删除计划.md §0.4.4 (C agent 设计) + §3 Layer 1.

守护契约:
  起 mossen 触发 1 个 prompt → ~/.mossen/projects/<projectId>/<sessionId>.jsonl 存在
  + jsonl 内容能找到 sessionId 字段且与文件名 stem 匹配 (UUID 兜底)

反测信号:
  - 删 sessionId 字段生成逻辑 → 文件用空名字 → R2 fail
  - 改 session 存储路径但没更新文件名 → 路径错 → R2 fail
  - session 改为内存态不落盘 → 文件不存在 → R2 fail

与现有 M12.2 区别:
  M12.2: 跨进程 resume + --continue 历史回放 (3 进程)
  R2:    sessionId 数据完整性 (1 进程, 低级稳定)
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log
from lib.mock_openai_provider import apply_mock_provider_env, mock_openai_provider

UUID_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{3,4}-[0-9a-f]{4}-[0-9a-f]{12}$"
)


def _make_env(ctx) -> dict:
    """补 MOSSEN_CONFIG_DIR (R-018 命名 bug 修)."""
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_START_BUILD"] = "never"
    return env


def _find_session_logs(home: Path) -> list[Path]:
    found = []
    for pattern in (
        "**/.mossen/transcripts/*.json",
        "**/transcripts/*.json",
        "**/projects/**/*.jsonl",
        "**/sessions/**/*.jsonl",
        "**/.mossen/**/*.jsonl",
    ):
        for p in home.glob(pattern):
            if p.is_file() and p not in found:
                found.append(p)
    return found


def _scan_session_id(path: Path) -> str | None:
    """从当前 transcript JSON 或旧 jsonl event 里找 session id 字段."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    if path.suffix == ".json":
        try:
            payload = json.loads(text)
        except json.JSONDecodeError:
            return None
        if isinstance(payload, dict):
            return payload.get("session_id") or payload.get("sessionId")
        return None
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            continue
        sid = ev.get("sessionId")
        if not sid:
            msg = ev.get("message", ev) if isinstance(ev, dict) else {}
            sid = msg.get("sessionId") if isinstance(msg, dict) else None
        if not sid and isinstance(ev, dict):
            metadata = ev.get("metadata") or {}
            if isinstance(metadata, dict):
                sid = metadata.get("sessionId")
        if sid:
            return sid
    return None


def case_session_id_persists() -> dict:
    ctx = make_fixture("R2_session_id")
    env = _make_env(ctx)

    fake_proj = ctx.root_dir / "fake_project"
    fake_proj.mkdir(parents=True, exist_ok=True)

    prompt = "请把以下字符串原样回复给我: R2_TEST_OK"

    with mock_openai_provider(model="r2-session-model") as (base_url, provider):
        apply_mock_provider_env(
            env,
            base_url,
            model="r2-session-model",
            name="R2 Session Mock",
        )
        ctx.env.update(env)
        proc = subprocess.run(
            [str(ROOT / "scripts" / "start-mossen.sh"), "--stdin"],
            input=prompt,
            env=env,
            capture_output=True,
            text=True,
            timeout=180,
            cwd=str(fake_proj),
        )
        provider_snapshot = provider.snapshot()
    write_command_log(ctx, ["mossen", "--stdin"], proc.stdout, proc.stderr, proc.returncode)

    session_logs = _find_session_logs(ctx.home_dir)

    # 1. 文件存在
    file_exists = len(session_logs) > 0

    # 2. 路径形如 .mossen/projects/<projectId>/<sessionId>.jsonl
    path_shape_ok = False
    sessionid_in_filename = None
    chosen = None
    for log in session_logs:
        parts = log.parts
        if "projects" in parts or "transcripts" in parts:
            stem = log.stem
            if UUID_RE.match(stem):
                path_shape_ok = True
                sessionid_in_filename = stem
                chosen = log
                break

    # 3. jsonl 非空
    file_nonempty = chosen is not None and chosen.stat().st_size > 0

    # 4. jsonl 中能找到 sessionId 字段
    sessionid_in_jsonl = _scan_session_id(chosen) if chosen else None

    # 5. 一致性 (jsonl 中 sessionId 与文件名 stem 一致, 或 jsonl 中无 sessionId 字段则 fallback 接受 UUID 文件名)
    consistency_ok = False
    consistency_mode = None
    if sessionid_in_jsonl is not None:
        consistency_ok = sessionid_in_jsonl == sessionid_in_filename
        consistency_mode = "strict"
    elif sessionid_in_filename is not None:
        consistency_ok = True
        consistency_mode = "weak (filename is UUID, no sessionId field per-event)"

    ok = (
        proc.returncode == 0
        and file_exists
        and path_shape_ok
        and file_nonempty
        and consistency_ok
    )

    return {
        "name": "session_id_persists_and_matches_filename",
        "ok": ok,
        "exit_code": proc.returncode,
        "file_exists": file_exists,
        "path_shape_ok": path_shape_ok,
        "file_nonempty": file_nonempty,
        "sessionid_in_filename": sessionid_in_filename,
        "sessionid_in_jsonl": sessionid_in_jsonl,
        "consistency_ok": consistency_ok,
        "consistency_mode": consistency_mode,
        "session_log_count": len(session_logs),
        "provider_request_count": provider_snapshot["request_count"],
        "provider_paths": provider_snapshot["paths"],
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
    res = _retry(case_session_id_persists)
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
                f"file_exists={res.get('file_exists')} "
                f"path_shape_ok={res.get('path_shape_ok')} "
                f"sessionid_match={res.get('consistency_ok')} "
                f"mode={res.get('consistency_mode')} "
                f"provider_req={res.get('provider_request_count')}"
            ),
        }],
    )

    summary = {
        "results": [res],
        "passed": 1 if res.get("ok") else 0,
        "total": 1,
        "design_note": (
            "R2: session jsonl 落盘 + 文件名 UUID + sessionId 字段一致 (或 weak-pass UUID 文件名)"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if res.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
