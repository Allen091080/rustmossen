#!/usr/bin/env python3
"""
M10.1 - long-running MCP tool completion e2e for the current Rust runner.

Contract:
  - the model emits an MCP tool_use;
  - Mossen executes a real slow MCP stdio tool and waits for completion;
  - the tool_result is fed back into the next model request;
  - the final assistant response is printed instead of the loop stopping after
    the tool call.

This test uses a local OpenAI-compatible mock server so it does not depend on a
real LLM service or the removed legacy run-mossen.sh wrapper.
"""

from __future__ import annotations

import json
import subprocess
import sys
import threading
import time
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

REAL_HOME = Path.home()
DEBUG_MOSSEN = ROOT / "target" / "debug" / "mossen"
MCP_SERVER_NAME = "harness_mock_slow_M10_1"
MCP_TOOL_FULL_NAME = f"mcp__{MCP_SERVER_NAME}__slow_M10_1"
SLOW_TAG = "SLOW_TAG_FROM_MOCK_M10_1"
SLOW_SLEEP_SECS = 10
MIN_EXPECTED_DURATION = 9.5
FINAL_MARKER = "FINAL_OK_M10_1"


def mossen_runner() -> str:
    if DEBUG_MOSSEN.exists() and DEBUG_MOSSEN.is_file():
        return str(DEBUG_MOSSEN)
    return str(ROOT / "scripts" / "start-mossen.sh")


class MockOpenAIState:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.requests: list[dict[str, Any]] = []
        self.tool_result_seen = False

    def record(self, path: str, body: bytes) -> int:
        try:
            parsed = json.loads(body.decode("utf-8")) if body else {}
        except json.JSONDecodeError:
            parsed = {"_decode_error": body.decode("utf-8", "replace")}
        with self.lock:
            self.requests.append({"path": path, "body": parsed})
            if SLOW_TAG in json.dumps(parsed, ensure_ascii=False):
                self.tool_result_seen = True
            return len(self.requests)

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "request_count": len(self.requests),
                "paths": [req["path"] for req in self.requests],
                "tool_result_seen": self.tool_result_seen,
            }


def write_sse(wfile, payload: dict[str, Any]) -> None:
    wfile.write(f"data: {json.dumps(payload)}\n\n".encode("utf-8"))
    wfile.flush()


def make_handler(state: MockOpenAIState):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            payload = json.dumps(
                {"object": "list", "data": [{"id": "m10-agent-loop-model", "object": "model"}]}
            ).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self) -> None:
            length = int(self.headers.get("Content-Length", "0") or "0")
            body = self.rfile.read(length) if length else b""
            request_index = state.record(self.path, body)
            if not self.path.endswith("/chat/completions"):
                self.send_response(404)
                self.end_headers()
                return

            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            try:
                if request_index == 1:
                    self._write_tool_call()
                else:
                    self._write_final()
            except (BrokenPipeError, ConnectionResetError):
                return
            finally:
                self.close_connection = True

        def _write_tool_call(self) -> None:
            write_sse(
                self.wfile,
                {
                    "id": "m10-long-tool-call",
                    "object": "chat.completion.chunk",
                    "choices": [
                        {
                            "index": 0,
                            "delta": {
                                "tool_calls": [
                                    {
                                        "index": 0,
                                        "id": "call_m10_1_slow",
                                        "type": "function",
                                        "function": {
                                            "name": MCP_TOOL_FULL_NAME,
                                            "arguments": json.dumps({"note": "heartbeat_test"}),
                                        },
                                    }
                                ]
                            },
                            "finish_reason": None,
                        }
                    ],
                },
            )
            write_sse(
                self.wfile,
                {
                    "id": "m10-long-tool-call",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 4},
                },
            )
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()

        def _write_final(self) -> None:
            write_sse(
                self.wfile,
                {
                    "id": "m10-long-final",
                    "object": "chat.completion.chunk",
                    "choices": [
                        {
                            "index": 0,
                            "delta": {
                                "content": f"{FINAL_MARKER}: slow tool result was received."
                            },
                            "finish_reason": None,
                        }
                    ],
                },
            )
            write_sse(
                self.wfile,
                {
                    "id": "m10-long-final",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 20, "completion_tokens": 8},
                },
            )
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()

        def log_message(self, _format: str, *_args: Any) -> None:
            return

    return Handler


@contextmanager
def mock_openai_server():
    state = MockOpenAIState()
    server = ThreadingHTTPServer(("127.0.0.1", 0), make_handler(state))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        host, port = server.server_address
        yield f"http://{host}:{port}", state
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def case_long_task_completes() -> dict:
    ctx = make_fixture("M10.1")
    mock_server_path = ROOT / "scripts" / "harness_mock_slow_mcp_server.py"

    mcp_config = {
        "mcpServers": {
            MCP_SERVER_NAME: {
                "type": "stdio",
                "command": "python3",
                "args": [str(mock_server_path)],
                "env": {"HARNESS_SLOW_SLEEP_SECS": str(SLOW_SLEEP_SECS)},
            }
        }
    }

    env = dict(ctx.env)
    env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    env.update(
        {
            "HARNESS_SLOW_SLEEP_SECS": str(SLOW_SLEEP_SECS),
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_MODEL": "m10-agent-loop-model",
            "MOSSEN_CODE_CUSTOM_NAME": "M10 Agent Loop Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-m10-local-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "30",
            "MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS": "30",
        }
    )

    prompt = (
        "Call the slow_M10_1 MCP tool once, wait for it to finish, then answer "
        f"with {FINAL_MARKER}."
    )

    with mock_openai_server() as (base_url, model_state):
        env["MOSSEN_CODE_CUSTOM_BASE_URL"] = base_url
        command = [
            mossen_runner(),
            "--stdin",
            "--mcp-config",
            json.dumps(mcp_config),
        ]
        t_start = time.monotonic()
        proc = subprocess.run(
            command,
            input=prompt,
            env=env,
            capture_output=True,
            text=True,
            timeout=180,
            cwd=str(ctx.root_dir),
        )
        duration = time.monotonic() - t_start
        server_snapshot = model_state.snapshot()

    write_command_log(ctx, command, proc.stdout, proc.stderr, proc.returncode)

    duration_ok = duration >= MIN_EXPECTED_DURATION
    final_marker_in_stdout = FINAL_MARKER in proc.stdout
    tool_result_seen_by_model = bool(server_snapshot["tool_result_seen"])
    request_count_ok = server_snapshot["request_count"] >= 2

    ok = (
        proc.returncode == 0
        and duration_ok
        and final_marker_in_stdout
        and tool_result_seen_by_model
        and request_count_ok
    )

    return {
        "name": "long_task_completes",
        "ok": ok,
        "exit_code": proc.returncode,
        "duration_secs": round(duration, 2),
        "duration_ok": duration_ok,
        "final_marker_in_stdout": final_marker_in_stdout,
        "tool_result_seen_by_model": tool_result_seen_by_model,
        "model_request_count": server_snapshot["request_count"],
        "model_request_paths": server_snapshot["paths"],
        "stdout_excerpt": proc.stdout[:500],
        "stderr_excerpt": proc.stderr[:500],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_long_task_completes()
    ctx = res.pop("_ctx")
    results = [res]

    write_assertions(
        ctx,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {
                "name": r["name"],
                "expected": True,
                "actual": r.get("ok"),
                "passed": r.get("ok"),
                "evidence": (
                    f"duration={r.get('duration_secs')}s duration_ok={r.get('duration_ok')} "
                    f"requests={r.get('model_request_count')} "
                    f"tool_result_seen={r.get('tool_result_seen_by_model')} "
                    f"final_stdout={r.get('final_marker_in_stdout')}"
                ),
            }
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            f"M10.1 uses current Rust runner and mock OpenAI backend. MCP sleeps "
            f"{SLOW_SLEEP_SECS}s; the next model request must contain {SLOW_TAG}."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
