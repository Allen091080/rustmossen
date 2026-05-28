#!/usr/bin/env python3
"""
M10.4 - Async Agent -> TaskOutput e2e for the current Rust runner.

This validates the parent loop mechanics that failed in real usage: the model
launches an Agent in background mode, Mossen returns an async task_id, the next
model turn calls TaskOutput with that exact id, and the final answer is based on
the completed child output.
"""

from __future__ import annotations

import json
import re
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
TASK_OUTPUT_TOOL_NAME = "TaskOutput"
PARENT_MARKER = "PARENT_OK_M10_4"
CHILD_MARKER = "ASYNC_CHILD_OK_M10_4"


def mossen_runner() -> str:
    return str(ROOT / "scripts" / "start-mossen.sh")


def iter_strings(value: Any):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from iter_strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from iter_strings(item)


def parse_json_string(value: str) -> Any | None:
    value = value.strip()
    if not value or value[0] not in "{[":
        return None
    try:
        return json.loads(value)
    except json.JSONDecodeError:
        return None


class MockOpenAIState:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.requests: list[dict[str, Any]] = []
        self.captured_task_id: str | None = None
        self.async_agent_result_seen = False
        self.child_request_seen = False
        self.task_output_result_seen = False

    def record(self, path: str, body: bytes) -> int:
        try:
            parsed = json.loads(body.decode("utf-8")) if body else {}
        except json.JSONDecodeError:
            parsed = {"_decode_error": body.decode("utf-8", "replace")}

        with self.lock:
            self.requests.append({"path": path, "body": parsed})
            for text in iter_strings(parsed):
                if "Mossen sub-agent launched by a parent session" in text:
                    self.child_request_seen = True
                if "async_launched" in text:
                    self.async_agent_result_seen = True
                    maybe_json = parse_json_string(text)
                    if isinstance(maybe_json, dict) and isinstance(
                        maybe_json.get("task_id"), str
                    ):
                        self.captured_task_id = maybe_json["task_id"]
                    elif self.captured_task_id is None:
                        match = re.search(r'"task_id"\s*:\s*"([^"]+)"', text)
                        if match:
                            self.captured_task_id = match.group(1)
                maybe_json = parse_json_string(text)
                if isinstance(maybe_json, dict) and maybe_json.get("retrieval_status"):
                    task = maybe_json.get("task")
                    if isinstance(task, dict) and CHILD_MARKER in str(task.get("output", "")):
                        self.task_output_result_seen = True
            return len(self.requests)

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "request_count": len(self.requests),
                "paths": [req["path"] for req in self.requests],
                "captured_task_id": self.captured_task_id,
                "async_agent_result_seen": self.async_agent_result_seen,
                "child_request_seen": self.child_request_seen,
                "task_output_result_seen": self.task_output_result_seen,
                "requests": self.requests,
            }


def write_sse(wfile, payload: dict[str, Any]) -> None:
    wfile.write(f"data: {json.dumps(payload)}\n\n".encode("utf-8"))
    wfile.flush()


def write_tool_call(wfile, call_id: str, name: str, args: dict[str, Any]) -> None:
    write_sse(
        wfile,
        {
            "id": call_id,
            "object": "chat.completion.chunk",
            "choices": [
                {
                    "index": 0,
                    "delta": {
                        "tool_calls": [
                            {
                                "index": 0,
                                "id": call_id,
                                "type": "function",
                                "function": {
                                    "name": name,
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
        wfile,
        {
            "id": call_id,
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 4},
        },
    )
    wfile.write(b"data: [DONE]\n\n")
    wfile.flush()


def write_text_final(wfile, text: str) -> None:
    write_sse(
        wfile,
        {
            "id": "m10-4-final",
            "object": "chat.completion.chunk",
            "choices": [
                {
                    "index": 0,
                    "delta": {"content": text},
                    "finish_reason": None,
                }
            ],
        },
    )
    write_sse(
        wfile,
        {
            "id": "m10-4-final",
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 20, "completion_tokens": 8},
        },
    )
    wfile.write(b"data: [DONE]\n\n")
    wfile.flush()


def make_handler(state: MockOpenAIState):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            payload = json.dumps(
                {"object": "list", "data": [{"id": "m10-async-agent-model", "object": "model"}]}
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
            snapshot = state.snapshot()
            is_child_request = any(
                "Mossen sub-agent launched by a parent session" in text
                for text in iter_strings(snapshot["requests"][-1]["body"])
            )
            if not self.path.endswith("/chat/completions"):
                self.send_response(404)
                self.end_headers()
                return

            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            try:
                if is_child_request:
                    write_text_final(
                        self.wfile,
                        f"{CHILD_MARKER}: child agent completed.",
                    )
                elif request_index == 1:
                    write_tool_call(
                        self.wfile,
                        "call_m10_4_agent",
                        AGENT_TOOL_NAME,
                        {
                            "description": "async marker",
                            "prompt": f"Return {CHILD_MARKER}.",
                            "subagent_type": "general-purpose",
                            "run_in_background": True,
                        },
                    )
                elif (
                    snapshot["captured_task_id"]
                    and not snapshot["task_output_result_seen"]
                ):
                    write_tool_call(
                        self.wfile,
                        "call_m10_4_task_output",
                        TASK_OUTPUT_TOOL_NAME,
                        {
                            "task_id": snapshot["captured_task_id"],
                            "block": True,
                            "timeout": 10000,
                        },
                    )
                elif snapshot["task_output_result_seen"]:
                    write_text_final(
                        self.wfile,
                        f"{PARENT_MARKER}: parent received async child output.",
                    )
                else:
                    write_text_final(
                        self.wfile,
                        "PARENT_MISSING_ASYNC_AGENT_TASK_ID_M10_4",
                    )
            except (BrokenPipeError, ConnectionResetError):
                return
            finally:
                self.close_connection = True

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


def case_async_agent_taskoutput_completes() -> dict:
    ctx = make_fixture("M10.4")
    env = dict(ctx.env)
    env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    env.update(
        {
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_MODEL": "m10-async-agent-model",
            "MOSSEN_CODE_CUSTOM_NAME": "M10 Async Agent Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-m10-async-local-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "30",
            "MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS": "30",
        }
    )

    prompt = (
        "Launch a background Agent, retrieve it with TaskOutput, then answer "
        f"with {PARENT_MARKER}."
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
    async_agent_result_seen = bool(server_snapshot["async_agent_result_seen"])
    child_request_seen = bool(server_snapshot["child_request_seen"])
    task_id_captured = bool(server_snapshot["captured_task_id"])
    task_output_result_seen = bool(server_snapshot["task_output_result_seen"])
    request_count_ok = server_snapshot["request_count"] >= 4

    ok = (
        proc.returncode == 0
        and parent_marker_in_stdout
        and async_agent_result_seen
        and child_request_seen
        and task_id_captured
        and task_output_result_seen
        and request_count_ok
    )

    return {
        "name": "async_agent_taskoutput_completes",
        "ok": ok,
        "exit_code": proc.returncode,
        "async_agent_result_seen": async_agent_result_seen,
        "child_request_seen": child_request_seen,
        "task_id_captured": task_id_captured,
        "captured_task_id": server_snapshot["captured_task_id"],
        "task_output_result_seen": task_output_result_seen,
        "parent_marker_in_stdout": parent_marker_in_stdout,
        "model_request_count": server_snapshot["request_count"],
        "model_request_paths": server_snapshot["paths"],
        "stdout_excerpt": proc.stdout[:800],
        "stderr_excerpt": proc.stderr[:800],
        "fixture_root": str(ctx.root_dir),
        "model_requests": str(requests_path),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_async_agent_taskoutput_completes()
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
                    f"async_agent_result_seen={r.get('async_agent_result_seen')} "
                    f"child_request_seen={r.get('child_request_seen')} "
                    f"task_id_captured={r.get('task_id_captured')} "
                    f"task_output_result_seen={r.get('task_output_result_seen')} "
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
            "M10.4 covers the async Agent launch plus TaskOutput retrieval path. "
            "It fails if the parent cannot extract the task_id or cannot see the "
            "child output before finalizing."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
