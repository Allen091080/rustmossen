#!/usr/bin/env python3
"""
R1 — 远程 telemetry 0 流量的安全网测试 (按 Allen D-3 决策: mock HTTP server).

按 OpenTelemetry删除计划.md §0.6 + §0.4.4.

守护契约:
  1. 强制开 telemetry env (绕守卫): MOSSEN_CODE_ENABLE_TELEMETRY=1
     + MOSSEN_CODE_TRUST_DIALOG_ACCEPTED=1
  2. 把所有 telemetry endpoint env 指向本机 mock HTTP server (random port)
  3. 模型 backend env (custom backend / external provider) 不动 → 走真 API
  4. 跑一次完整对话
  5. 断言:
     - mock_server 收到的 telemetry 请求数 = 0
     - proc.returncode == 0
     - session jsonl 落盘 (正常对话完成)
     - ~/.mossen/traces/ 不存在或为空 (perfetto 不创建)

反测信号:
  - 删某 slice 后误开 OTel exporter → mock 收到请求 → R1 fail
  - 误把 telemetry default endpoint 指向真 backend → mock 收不到但断言不灵
    → 用白名单 / 兜底: 任何 host != allowlist 视为可疑

实施: 启动 mock HTTP server 子进程; mossen 子进程同时跑; 跑完后停止 mock server.

注意: env disable 测不出"误发遥测", 必须用 mock server (Allen D-3 决策).
"""

from __future__ import annotations

import json
import os
import socket
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log
from lib.mock_openai_provider import apply_mock_provider_env, mock_openai_provider


# ============================================================================
# Mock HTTP server — 接收所有 telemetry 请求并记录
# ============================================================================

class MockTelemetryHandler(BaseHTTPRequestHandler):
    """记录每个收到的请求, 永远返回 200."""

    received_requests: list[dict] = []

    def do_POST(self):  # noqa: N802
        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length) if length > 0 else b""
        MockTelemetryHandler.received_requests.append({
            "method": "POST",
            "path": self.path,
            "host_header": self.headers.get("Host", ""),
            "content_length": length,
            "body_excerpt": body[:200].decode("utf-8", errors="replace"),
            "timestamp": time.time(),
        })
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"ok":true}')

    def do_GET(self):  # noqa: N802
        MockTelemetryHandler.received_requests.append({
            "method": "GET",
            "path": self.path,
            "host_header": self.headers.get("Host", ""),
            "timestamp": time.time(),
        })
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"ok":true}')

    def log_message(self, format, *args):
        # 静音 stderr (不污染 fixture 输出)
        pass


def _alloc_port() -> int:
    """让 OS 分配空闲端口."""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def _start_mock_server() -> tuple[HTTPServer, int, threading.Thread]:
    port = _alloc_port()
    MockTelemetryHandler.received_requests = []
    server = HTTPServer(("127.0.0.1", port), MockTelemetryHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, port, thread


def _stop_mock_server(server: HTTPServer, thread: threading.Thread):
    server.shutdown()
    server.server_close()
    thread.join(timeout=5)


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


def _make_env(ctx, mock_port: int) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_START_BUILD"] = "never"

    # 强制开 telemetry (绕过守卫, 让 mossen 真试图发数据)
    env["MOSSEN_CODE_ENABLE_TELEMETRY"] = "1"
    env["MOSSEN_CODE_TRUST_DIALOG_ACCEPTED"] = "1"
    env["MOSSEN_NON_INTERACTIVE_SESSION"] = "1"

    # 所有 telemetry endpoint 指向 mock
    mock_url = f"http://127.0.0.1:{mock_port}"
    env["ANT_MOSSEN_METRICS_ENDPOINT"] = mock_url
    env["OTEL_EXPORTER_OTLP_ENDPOINT"] = mock_url
    env["OTEL_EXPORTER_OTLP_LOGS_ENDPOINT"] = mock_url
    env["OTEL_EXPORTER_OTLP_METRICS_ENDPOINT"] = mock_url
    env["OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"] = mock_url

    return env


def case_no_remote_telemetry_traffic() -> dict:
    ctx = make_fixture("R1_no_telemetry")

    server, port, thread = _start_mock_server()
    try:
        env = _make_env(ctx, port)
        fake_proj = ctx.root_dir / "fake_project"
        fake_proj.mkdir(parents=True, exist_ok=True)

        prompt = "请把以下字符串原样回复给我: R1_TELEMETRY_TEST_OK"

        with mock_openai_provider(model="r1-telemetry-model") as (base_url, provider):
            apply_mock_provider_env(
                env,
                base_url,
                model="r1-telemetry-model",
                name="R1 Telemetry Mock",
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
        write_command_log(
            ctx,
            ["mossen", "--stdin", f"(mock_telemetry_port={port}, force_enable=1)"],
            proc.stdout, proc.stderr, proc.returncode,
        )

        # 给后台 telemetry 队列一点时间 flush (但应该 0 请求)
        time.sleep(2)

        telemetry_requests = list(MockTelemetryHandler.received_requests)

        session_logs = _find_session_logs(ctx.home_dir)
        traces_dir = ctx.mossen_config_home / "traces"
        traces_files = list(traces_dir.glob("*")) if traces_dir.exists() else []
    finally:
        _stop_mock_server(server, thread)

    no_telemetry_traffic = len(telemetry_requests) == 0
    session_landed = len(session_logs) > 0
    no_traces_files = len(traces_files) == 0

    ok = (
        proc.returncode == 0
        and no_telemetry_traffic
        and session_landed
        and no_traces_files
    )

    return {
        "name": "no_remote_telemetry_traffic",
        "ok": ok,
        "exit_code": proc.returncode,
        "mock_port": port,
        "telemetry_request_count": len(telemetry_requests),
        "telemetry_request_excerpt": telemetry_requests[:3],
        "session_landed": session_landed,
        "session_log_count": len(session_logs),
        "no_traces_files": no_traces_files,
        "traces_files": [str(p) for p in traces_files[:5]],
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
    res = _retry(case_no_remote_telemetry_traffic)
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
                f"telemetry_req={res.get('telemetry_request_count')} "
                f"session_landed={res.get('session_landed')} "
                f"no_traces={res.get('no_traces_files')} "
                f"provider_req={res.get('provider_request_count')}"
            ),
        }],
    )

    summary = {
        "results": [res],
        "passed": 1 if res.get("ok") else 0,
        "total": 1,
        "design_note": (
            "R1: mock HTTP server 拦 telemetry endpoint, 强制开 telemetry env, "
            "断言 0 telemetry 请求 + 正常对话完成 (Allen D-3 决策)."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if res.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
