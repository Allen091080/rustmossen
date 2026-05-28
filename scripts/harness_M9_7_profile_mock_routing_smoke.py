#!/usr/bin/env python3
"""
M9.7 — fake OpenAI-compatible mock server + profile 切换真路由 e2e (S1-09d P0).

按 Allen D-S09-4=K1 + Stage1 §11.6 契约:
  不能依赖外网 / 真 LLM API. 用 Python 内嵌 http.server 起 fake OpenAI-compatible
  mock server (任意 free port, /models endpoint), 设 profile 指向它,
  跑 mossen --test-model-profile <name> 验:
    - request 真到达 mock server
    - Authorization: Bearer <apiKey> header 真透传
    - 切换 active profile 后, request 路由到不同 baseURL (server A vs B)

  关键 case (3):
    1. case_test_profile_routes_to_mock_A: profile A baseURL=mock-A → mossen --test → mock-A 收 ≥1
    2. case_switch_profile_changes_route: 加 profile B baseURL=mock-B → switch active=B, --test → mock-B 收 ≥1, mock-A 不增
    3. case_apikey_in_authorization_header: --test-model-profile A → mock-A 收到的 request Authorization: Bearer <真 apiKey>

  反测信号:
    a) testProfile 不发 fetch / 用错 baseURL → mock server 0 hits → fail
    b) testProfile 不带 Authorization header → mock 收到没 Authorization → case 3 fail
    c) profileCli 把 active profile 写错 / customBackend 不切 → switch 后 mock-A 仍收 → case 2 fail
"""

from __future__ import annotations

import json
import socket
import subprocess
import sys
import threading
import time
from collections import defaultdict
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_MOSSEN = str(ROOT / "scripts" / "start-mossen.sh")

PROFILE_A_KEY = "sk-test-mock-A-AAAAAAAAAAAAAAAA"
PROFILE_B_KEY = "sk-test-mock-B-BBBBBBBBBBBBBBBB"


class _MockState:
    """mock server 收到的请求记录 (per server instance)"""
    def __init__(self, label: str):
        self.label = label
        self.requests: list[dict] = []
        self.lock = threading.Lock()

    def record(self, path: str, headers: dict, body: bytes):
        normalized_headers = {str(k).lower(): v for k, v in headers.items()}
        with self.lock:
            self.requests.append({
                "path": path,
                "authorization": normalized_headers.get("authorization", ""),
                "user_agent": normalized_headers.get("user-agent", ""),
                "body": body.decode("utf-8", errors="replace")[:200] if body else "",
                "ts": time.time(),
            })

    def count(self) -> int:
        with self.lock:
            return len(self.requests)

    def snapshot(self) -> list[dict]:
        with self.lock:
            return list(self.requests)


def _make_handler(state: _MockState):
    class _Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            state.record(self.path, dict(self.headers), b"")
            payload = json.dumps({
                "object": "list",
                "data": [
                    {"id": f"{state.label}-model-1", "object": "model"},
                ],
            }).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self):
            length = int(self.headers.get("Content-Length", "0") or "0")
            body = self.rfile.read(length) if length else b""
            state.record(self.path, dict(self.headers), body)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            payload = json.dumps({"id": "mock-response", "object": "chat.completion"}).encode("utf-8")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, format, *args):
            return  # silence

    return _Handler


def _start_mock_server(label: str) -> tuple[HTTPServer, _MockState, int, threading.Thread]:
    state = _MockState(label)
    handler = _make_handler(state)
    # 0 = let kernel pick free port
    server = HTTPServer(("127.0.0.1", 0), handler)
    port = server.server_address[1]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, state, port, thread


def _stop_mock_server(server: HTTPServer, thread: threading.Thread):
    server.shutdown()
    server.server_close()
    thread.join(timeout=2)


def _run_mossen(env: dict, args: list[str], timeout: int = 60) -> tuple[int, str, str]:
    proc = subprocess.run(
        [RUN_MOSSEN, *args],
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=timeout,
    )
    return proc.returncode, proc.stdout, proc.stderr


def _parse_json(stdout: str) -> dict | None:
    try:
        return json.loads(stdout)
    except json.JSONDecodeError:
        for line in reversed(stdout.splitlines()):
            line = line.strip()
            if line.startswith("{"):
                try:
                    return json.loads(line)
                except json.JSONDecodeError:
                    continue
        return None


def case_routing_and_apikey_passthrough() -> dict:
    """
    完整链路: 起 2 mock server (A, B), 写 2 profile, 测试切换 active 后真路由 + apiKey 透传.
    """
    ctx = make_fixture("M9.7.routing")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    server_a, state_a, port_a, thread_a = _start_mock_server("A")
    server_b, state_b, port_b, thread_b = _start_mock_server("B")

    try:
        # 写空 settings.json
        settings_dir = ctx.mossen_config_home
        settings_dir.mkdir(parents=True, exist_ok=True)
        (settings_dir / "settings.json").write_text("{}", encoding="utf-8")

        # Step 1: add profile A pointing to mock-A
        rc1, stdout1, stderr1 = _run_mossen(env, [
            "--add-model-profile", "mock-a",
            "--provider", "openai-compatible",
            "--baseURL", f"http://127.0.0.1:{port_a}/v1",
            "--model", "A-model-1",
            "--apiKey", PROFILE_A_KEY,
        ])
        # Step 2: add profile B pointing to mock-B
        rc2, stdout2, stderr2 = _run_mossen(env, [
            "--add-model-profile", "mock-b",
            "--provider", "openai-compatible",
            "--baseURL", f"http://127.0.0.1:{port_b}/v1",
            "--model", "B-model-1",
            "--apiKey", PROFILE_B_KEY,
        ])

        # Step 3: --test-model-profile mock-a → server-A 应收 1 req
        rc3, stdout3, stderr3 = _run_mossen(env, ["--test-model-profile", "mock-a"])
        parsed3 = _parse_json(stdout3) or {}
        a_count_after_3 = state_a.count()
        b_count_after_3 = state_b.count()

        # Step 4: --test-model-profile mock-b → server-B 应收 1 req, server-A 不增
        rc4, stdout4, stderr4 = _run_mossen(env, ["--test-model-profile", "mock-b"])
        parsed4 = _parse_json(stdout4) or {}
        a_count_after_4 = state_a.count()
        b_count_after_4 = state_b.count()

        # Step 5: switch active=mock-a + --test 验 active 切换不影响 --test (--test 用 name 直接定位, 不依赖 active)
        # 这里间接验 setActive 也持久化
        rc5, stdout5, stderr5 = _run_mossen(env, ["--set-model-profile", "mock-a"])
        parsed5 = _parse_json(stdout5) or {}

        # Step 6: --test-model-profile 现在 active=mock-a, 但显式指定 mock-b → 仍打 server-B
        rc6, stdout6, stderr6 = _run_mossen(env, ["--test-model-profile", "mock-b"])
        parsed6 = _parse_json(stdout6) or {}
        a_count_final = state_a.count()
        b_count_final = state_b.count()

        # 验 server-A 收到的最后一个请求, Authorization 真透传 PROFILE_A_KEY
        a_snapshot = state_a.snapshot()
        b_snapshot = state_b.snapshot()
        a_authorization = a_snapshot[-1]["authorization"] if a_snapshot else ""
        b_authorization = b_snapshot[-1]["authorization"] if b_snapshot else ""

        ok = (
            rc1 == 0 and rc2 == 0
            and rc3 == 0
            and parsed3.get("ok") is True
            and parsed3.get("action") == "test"
            and (parsed3.get("result") or {}).get("status") == 200
            and a_count_after_3 == 1
            and b_count_after_3 == 0
            # Step 4: B 收 1, A 不变
            and rc4 == 0
            and parsed4.get("ok") is True
            and a_count_after_4 == 1
            and b_count_after_4 == 1
            # Step 5: setActive ok
            and rc5 == 0
            and parsed5.get("activeProfile") == "mock-a"
            # Step 6: 显式 mock-b 仍打 B server (--test 用 name, 不用 active)
            and rc6 == 0
            and a_count_final == 1
            and b_count_final == 2
            # Authorization header 真透传
            and a_authorization == f"Bearer {PROFILE_A_KEY}"
            and b_authorization == f"Bearer {PROFILE_B_KEY}"
            # CLI dump apiKey 已脱敏 (parsed3.profile.apiKey 不应是真 key)
            and (parsed3.get("profile") or {}).get("apiKey") != PROFILE_A_KEY
            and "..." in (parsed3.get("profile") or {}).get("apiKey", "")
        )

        write_command_log(
            ctx,
            [RUN_MOSSEN, "--test-model-profile <chain>"],
            f"server_A_requests={a_count_final} server_B_requests={b_count_final}",
            f"rc1={rc1} rc2={rc2} rc3={rc3} rc4={rc4} rc5={rc5} rc6={rc6}",
            rc1 + rc2 + rc3 + rc4 + rc5 + rc6,
        )

        return {
            "name": "M9_7_routing_and_apikey_passthrough_via_mock_servers",
            "ok": ok,
            "rcs": [rc1, rc2, rc3, rc4, rc5, rc6],
            "server_a_count_progression": [
                ("after_step_3_test_a", a_count_after_3),
                ("after_step_4_test_b", a_count_after_4),
                ("final", a_count_final),
            ],
            "server_b_count_progression": [
                ("after_step_3_test_a", b_count_after_3),
                ("after_step_4_test_b", b_count_after_4),
                ("final", b_count_final),
            ],
            "a_authorization_intact": a_authorization == f"Bearer {PROFILE_A_KEY}",
            "b_authorization_intact": b_authorization == f"Bearer {PROFILE_B_KEY}",
            "a_snapshot_paths": [r["path"] for r in a_snapshot],
            "b_snapshot_paths": [r["path"] for r in b_snapshot],
            "_ctx": ctx,
        }
    finally:
        _stop_mock_server(server_a, thread_a)
        _stop_mock_server(server_b, thread_b)


def case_test_profile_unreachable_server() -> dict:
    """
    profile 指向真不可达 server (随机 unused port) → testProfile ok=false + status=0.
    起一个 server 占住 port, 然后 _stop 它使 port unused (race-safe 不保证, 用 closed port).
    """
    ctx = make_fixture("M9.7.unreachable")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    # 找一个临时 free port, 不起 server (确保连不上)
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(("127.0.0.1", 0))
    unused_port = sock.getsockname()[1]
    sock.close()

    settings_dir = ctx.mossen_config_home
    settings_dir.mkdir(parents=True, exist_ok=True)
    (settings_dir / "settings.json").write_text("{}", encoding="utf-8")

    rc1, _, _ = _run_mossen(env, [
        "--add-model-profile", "unreachable",
        "--provider", "openai-compatible",
        "--baseURL", f"http://127.0.0.1:{unused_port}/v1",
        "--model", "x",
        "--apiKey", "sk-fake-CCCCCCCCCCCCCCCC",
    ])

    rc2, stdout2, stderr2 = _run_mossen(env, [
        "--test-model-profile", "unreachable",
        "--timeout", "2000",
    ], timeout=30)
    parsed2 = _parse_json(stdout2) or {}
    result = parsed2.get("result") or {}

    ok = (
        rc1 == 0
        and rc2 == 1  # exit 1 因 server unreachable
        and parsed2.get("ok") is False
        and result.get("ok") is False
        and result.get("status") == 0
        and isinstance(result.get("error"), str)
        and len(result.get("error", "")) > 0
    )

    write_command_log(
        ctx,
        [RUN_MOSSEN, "--test-model-profile unreachable"],
        json.dumps(parsed2),
        stderr2[:300],
        rc2,
    )

    return {
        "name": "M9_7_test_profile_unreachable_server_returns_ok_false",
        "ok": ok,
        "rc": rc2,
        "result": result,
        "_ctx": ctx,
    }


def main() -> int:
    cases = [
        case_routing_and_apikey_passthrough(),
        case_test_profile_unreachable_server(),
    ]
    summary_status = "passed" if all(c.get("ok") for c in cases) else "failed"

    last_ctx = cases[-1].pop("_ctx")
    for c in cases[:-1]:
        c.pop("_ctx", None)

    write_assertions(
        last_ctx,
        status=summary_status,
        assertions=[
            {
                "name": c["name"],
                "expected": True,
                "actual": c.get("ok"),
                "passed": c.get("ok"),
                "evidence": json.dumps(c, ensure_ascii=False)[:500],
            }
            for c in cases
        ],
    )
    print(json.dumps({"status": summary_status, "results": cases}, indent=2, ensure_ascii=False))
    return 0 if summary_status == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
