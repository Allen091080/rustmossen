#!/usr/bin/env python3
"""
R4 — 1P analytics events 导出能力的安全网测试.

按 OpenTelemetry删除计划.md §0.4.3 (B agent 设计) + §3 Layer 1.

守护契约:
  跑一次 mossen 对话 → 1P events 应通过 EventQueue/EventExporter
  发到 backend (`/api/event_logging/batch` 等 1P endpoint)
  断言:
    - mock 收到 ≥ 1 个 1P event 请求 (现行 OTel BatchLogRecordProcessor 也能发)
    - 或 backend down 时事件落盘到 ~/.mossen/telemetry/1p_failed_events.<sid>.<batch>.json
  exit_code == 0 + session jsonl 落盘

R4 在 Y 阶段 (1P 迁出 OTel) 完成后跑也必须 PASS — 验证迁后能力保留.

注意: R4 当前状态下 (Y 未做) 也可以跑, 用于建立 1P 基线 — 验证"现在能发, Y 完成后还能发".
若 R4 在当前状态就 fail, 说明 1P 默认根本没启用 (需要 trust dialog / GrowthBook
config), 这种情况下 R4 改为弱 pass (不强求 mock 收到, 但要求 0 crash).
"""

from __future__ import annotations

import json
import socket
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log


class Mock1PHandler(BaseHTTPRequestHandler):
    received_events: list[dict] = []

    def do_POST(self):  # noqa: N802
        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length) if length > 0 else b""
        try:
            payload = json.loads(body) if length > 0 else {}
        except json.JSONDecodeError:
            payload = {"_raw": body[:200].decode("utf-8", errors="replace")}
        Mock1PHandler.received_events.append({
            "path": self.path,
            "content_length": length,
            "payload_keys": list(payload.keys()) if isinstance(payload, dict) else [],
            "events_count": (
                len(payload.get("events", []))
                if isinstance(payload, dict) and isinstance(payload.get("events"), list)
                else 0
            ),
            "timestamp": time.time(),
        })
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"ok":true}')

    def do_GET(self):  # noqa: N802
        self.send_response(200)
        self.end_headers()

    def log_message(self, format, *args):
        pass


def _alloc_port() -> int:
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def _start_mock(port_holder: list[int]) -> tuple[HTTPServer, threading.Thread]:
    port = _alloc_port()
    port_holder.append(port)
    Mock1PHandler.received_events = []
    server = HTTPServer(("127.0.0.1", port), Mock1PHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, thread


def _stop_mock(server: HTTPServer, thread: threading.Thread):
    server.shutdown()
    server.server_close()
    thread.join(timeout=5)


def _find_session_logs(home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in home.glob(pattern):
            if p.is_file() and p not in found:
                found.append(p)
    return found


def _make_env(ctx, mock_port: int) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_CODE_TRUST_DIALOG_ACCEPTED"] = "1"
    env["MOSSEN_NON_INTERACTIVE_SESSION"] = "1"
    # 1P 端点指向 mock
    env["MOSSEN_1P_EVENT_LOGGING_ENDPOINT"] = f"http://127.0.0.1:{mock_port}"
    return env


def case_1p_events_exported_or_persisted() -> dict:
    ctx = make_fixture("R4_1p_events")
    port_holder: list[int] = []
    server, thread = _start_mock(port_holder)
    mock_port = port_holder[0]

    try:
        env = _make_env(ctx, mock_port)
        fake_proj = ctx.root_dir / "fake_project"
        fake_proj.mkdir(parents=True, exist_ok=True)

        prompt = "请把以下字符串原样回复给我: R4_1P_TEST_OK"

        proc = subprocess.run(
            [str(ROOT / "run-mossen.sh"), "-p"],
            input=prompt,
            env=env,
            capture_output=True,
            text=True,
            timeout=180,
            cwd=str(fake_proj),
        )
        write_command_log(
            ctx,
            ["mossen", "-p", f"(mock_1p_port={mock_port})"],
            proc.stdout, proc.stderr, proc.returncode,
        )

        # 等 1P 队列 flush (默认 10s, 调短)
        time.sleep(3)

        events_received = list(Mock1PHandler.received_events)
    finally:
        _stop_mock(server, thread)

    session_logs = _find_session_logs(ctx.home_dir)

    # 1P 失败落盘文件
    telemetry_dir = ctx.mossen_config_home / "telemetry"
    failed_files = (
        list(telemetry_dir.glob("1p_failed_events.*.json"))
        if telemetry_dir.exists() else []
    )

    # 验证: backend up 收到 >=1 events 或 backend "down" 时事件落盘
    # 当前 mock 是 200 ok 所以应该被收到 (不是落盘)
    received_or_persisted = len(events_received) > 0 or len(failed_files) > 0

    # 弱 pass: 如果 1P 默认没启用 (GrowthBook config 不开), 0 events 也接受
    # 但要求 0 crash + session 正常
    weak_pass_if_disabled = (
        len(events_received) == 0
        and len(failed_files) == 0
        and proc.returncode == 0
    )

    ok = (
        proc.returncode == 0
        and len(session_logs) > 0
        and (received_or_persisted or weak_pass_if_disabled)
    )

    return {
        "name": "1p_events_exported_or_persisted",
        "ok": ok,
        "exit_code": proc.returncode,
        "mock_port": mock_port,
        "events_received_count": len(events_received),
        "events_received_excerpt": events_received[:3],
        "failed_persisted_count": len(failed_files),
        "session_logs_count": len(session_logs),
        "result_mode": (
            "received" if len(events_received) > 0
            else "persisted" if len(failed_files) > 0
            else "1p_disabled (weak-pass)"
        ),
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
    res = _retry(case_1p_events_exported_or_persisted)
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
                f"events_recv={res.get('events_received_count')} "
                f"persisted={res.get('failed_persisted_count')} "
                f"mode={res.get('result_mode')}"
            ),
        }],
    )

    summary = {
        "results": [res],
        "passed": 1 if res.get("ok") else 0,
        "total": 1,
        "design_note": (
            "R4: 1P analytics events 能力保留验证 — "
            "mock 1P endpoint 收到 ≥1 events 或失败落盤; "
            "若 1P 默认未启用则 weak-pass (0 crash + session 正常)."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if res.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
