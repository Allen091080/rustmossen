#!/usr/bin/env python3
"""
M10.3 - Agent tool result handoff e2e for the current Rust runner.

This validates the parent loop mechanics around the Agent tool: model emits
Agent tool_use, Mossen executes it, feeds the non-error tool_result back into
the next model request, and the parent produces a final visible response.
"""

from __future__ import annotations

import json
import subprocess
import sys
import threading
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

REAL_HOME = Path.home()
AGENT_TOOL_NAME = "Agent"
PARENT_MARKER = "PARENT_OK_M10_3"
AGENT_RESULT_MARKER = "Agent completed task"
CHILD_MARKER = "child_marker_M10_3"


def mossen_runner() -> str:
    return str(ROOT / "scripts" / "start-mossen.sh")


class MockOpenAIState:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.requests: list[dict[str, Any]] = []
        self.agent_result_seen = False
        self.child_request_seen = False

    def record(self, path: str, body: bytes) -> tuple[int, bool]:
        try:
            parsed = json.loads(body.decode("utf-8")) if body else {}
        except json.JSONDecodeError:
            parsed = {"_decode_error": body.decode("utf-8", "replace")}
        body_text = json.dumps(parsed, ensure_ascii=False)
        is_child_request = "Mossen sub-agent launched by a parent session" in body_text
        with self.lock:
            self.requests.append({"path": path, "body": parsed})
            if is_child_request:
                self.child_request_seen = True
            if AGENT_RESULT_MARKER in body_text:
                self.agent_result_seen = True
            return len(self.requests), is_child_request

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "request_count": len(self.requests),
                "paths": [req["path"] for req in self.requests],
                "agent_result_seen": self.agent_result_seen,
                "child_request_seen": self.child_request_seen,
                "requests": self.requests,
            }


def write_sse(wfile, payload: dict[str, Any]) -> None:
    wfile.write(f"data: {json.dumps(payload)}\n\n".encode("utf-8"))
    wfile.flush()


def make_handler(state: MockOpenAIState):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            payload = json.dumps(
                {"object": "list", "data": [{"id": "m10-agent-tool-model", "object": "model"}]}
            ).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self) -> None:
            length = int(self.headers.get("Content-Length", "0") or "0")
            body = self.rfile.read(length) if length else b""
            request_index, is_child_request = state.record(self.path, body)
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
                    self._write_agent_call()
                elif is_child_request:
                    self._write_child_result()
                elif state.snapshot()["agent_result_seen"]:
                    self._write_final()
                else:
                    self._write_missing_result()
            except (BrokenPipeError, ConnectionResetError):
                return
            finally:
                self.close_connection = True

        def _write_agent_call(self) -> None:
            args = {
                "description": "read marker",
                "prompt": "Return child_marker_M10_3 from the supplied task context.",
                "subagent_type": "general-purpose",
                "run_in_background": False,
            }
            write_sse(
                self.wfile,
                {
                    "id": "m10-agent-tool-call",
                    "object": "chat.completion.chunk",
                    "choices": [
                        {
                            "index": 0,
                            "delta": {
                                "tool_calls": [
                                    {
                                        "index": 0,
                                        "id": "call_m10_3_agent",
                                        "type": "function",
                                        "function": {
                                            "name": AGENT_TOOL_NAME,
                                            "arguments": json.dumps(args),
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
                    "id": "m10-agent-tool-call",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 4},
                },
            )
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()

        def _write_child_result(self) -> None:
            write_sse(
                self.wfile,
                {
                    "id": "m10-agent-child",
                    "object": "chat.completion.chunk",
                    "choices": [
                        {
                            "index": 0,
                            "delta": {"content": f"{AGENT_RESULT_MARKER}: {CHILD_MARKER}"},
                            "finish_reason": None,
                        }
                    ],
                },
            )
            write_sse(
                self.wfile,
                {
                    "id": "m10-agent-child",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 12, "completion_tokens": 5},
                },
            )
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()

        def _write_missing_result(self) -> None:
            write_sse(
                self.wfile,
                {
                    "id": "m10-agent-missing-result",
                    "object": "chat.completion.chunk",
                    "choices": [
                        {
                            "index": 0,
                            "delta": {
                                "content": "PARENT_MISSING_AGENT_RESULT_M10_3: Agent result was absent."
                            },
                            "finish_reason": None,
                        }
                    ],
                },
            )
            write_sse(
                self.wfile,
                {
                    "id": "m10-agent-missing-result",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 20, "completion_tokens": 8},
                },
            )
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()

        def _write_final(self) -> None:
            write_sse(
                self.wfile,
                {
                    "id": "m10-agent-final",
                    "object": "chat.completion.chunk",
                    "choices": [
                        {
                            "index": 0,
                            "delta": {
                                "content": f"{PARENT_MARKER}: parent received Agent result."
                            },
                            "finish_reason": None,
                        }
                    ],
                },
            )
            write_sse(
                self.wfile,
                {
                    "id": "m10-agent-final",
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


def case_nested_subtask_completes() -> dict:
    ctx = make_fixture("M10.3")
    env = dict(ctx.env)
    env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    env.update(
        {
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_MODEL": "m10-agent-tool-model",
            "MOSSEN_CODE_CUSTOM_NAME": "M10 Agent Tool Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-m10-local-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "30",
            "MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS": "30",
        }
    )

    prompt = (
        "Use the Agent tool once, wait for the result, then answer with "
        f"{PARENT_MARKER}."
    )

    with mock_openai_server() as (base_url, model_state):
        env["MOSSEN_CODE_CUSTOM_BASE_URL"] = base_url
        command = [mossen_runner(), "--stdin"]
        proc = subprocess.run(
            command,
            input=prompt,
            env=env,
            capture_output=True,
            text=True,
            timeout=120,
            cwd=str(ctx.root_dir),
        )
        server_snapshot = model_state.snapshot()

    write_command_log(ctx, command, proc.stdout, proc.stderr, proc.returncode)
    requests_path = ctx.artifacts_dir / "model_requests.json"
    requests_path.write_text(json.dumps(server_snapshot["requests"], indent=2, ensure_ascii=False))

    parent_marker_in_stdout = PARENT_MARKER in proc.stdout
    agent_result_seen_by_model = bool(server_snapshot["agent_result_seen"])
    child_request_seen = bool(server_snapshot["child_request_seen"])
    request_count_ok = server_snapshot["request_count"] >= 2

    ok = (
        proc.returncode == 0
        and parent_marker_in_stdout
        and agent_result_seen_by_model
        and child_request_seen
        and request_count_ok
    )

    return {
        "name": "nested_subtask_completes",
        "ok": ok,
        "exit_code": proc.returncode,
        "agent_result_seen_by_model": agent_result_seen_by_model,
        "child_request_seen": child_request_seen,
        "parent_marker_in_stdout": parent_marker_in_stdout,
        "model_request_count": server_snapshot["request_count"],
        "model_request_paths": server_snapshot["paths"],
        "stdout_excerpt": proc.stdout[:500],
        "stderr_excerpt": proc.stderr[:500],
        "fixture_root": str(ctx.root_dir),
        "model_requests": str(requests_path),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_nested_subtask_completes()
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
                    f"requests={r.get('model_request_count')} "
                    f"agent_result_seen={r.get('agent_result_seen_by_model')} "
                    f"child_request_seen={r.get('child_request_seen')} "
                    f"parent_marker={r.get('parent_marker_in_stdout')}"
                ),
            }
            for r in results
        ],
        extra_artifacts={"model_requests": str(ctx.artifacts_dir / "model_requests.json")},
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M10.3 uses current Rust runner and mock OpenAI backend. Agent tool_result "
            "must be present in the second model request before the parent final response."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
