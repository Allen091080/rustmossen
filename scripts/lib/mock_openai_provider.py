"""Shared local OpenAI-compatible streaming provider for harness smokes."""

from __future__ import annotations

import json
import re
import threading
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Iterator


MARKER_RE = re.compile(r"\b(?:R|M)\d+(?:[._]\d+)?_[A-Za-z0-9_]+\b")


def iter_strings(value: Any):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from iter_strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from iter_strings(item)


def _write_sse(wfile, payload: dict[str, Any]) -> None:
    wfile.write(f"data: {json.dumps(payload)}\n\n".encode("utf-8"))
    wfile.flush()


def _write_final(wfile, text: str) -> None:
    _write_sse(
        wfile,
        {
            "id": "harness-final",
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
    _write_sse(
        wfile,
        {
            "id": "harness-final",
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 16, "completion_tokens": 4},
        },
    )
    wfile.write(b"data: [DONE]\n\n")
    wfile.flush()


def _write_bash_tool_call(wfile, marker: str) -> None:
    _write_sse(
        wfile,
        {
            "id": "harness-bash-tool",
            "object": "chat.completion.chunk",
            "choices": [
                {
                    "index": 0,
                    "delta": {
                        "tool_calls": [
                            {
                                "index": 0,
                                "id": "call_harness_bash",
                                "type": "function",
                                "function": {
                                    "name": "Bash",
                                    "arguments": json.dumps(
                                        {"command": f"sleep 0.1 && echo {marker}"}
                                    ),
                                },
                            }
                        ]
                    },
                    "finish_reason": None,
                }
            ],
        },
    )
    _write_sse(
        wfile,
        {
            "id": "harness-bash-tool",
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}],
            "usage": {"prompt_tokens": 20, "completion_tokens": 8},
        },
    )
    wfile.write(b"data: [DONE]\n\n")
    wfile.flush()


class MockOpenAIProvider:
    def __init__(self, model: str = "harness-local-model") -> None:
        self.model = model
        self.lock = threading.Lock()
        self.requests: list[dict[str, Any]] = []
        self.tool_call_sent = False

    def record(self, path: str, body: bytes) -> dict[str, Any]:
        try:
            parsed = json.loads(body.decode("utf-8")) if body else {}
        except json.JSONDecodeError:
            parsed = {"_decode_error": body.decode("utf-8", "replace")}
        body_text = "\n".join(iter_strings(parsed))
        markers = MARKER_RE.findall(body_text)
        with self.lock:
            self.requests.append({"path": path, "body": parsed, "markers": markers})
            should_call_bash = (
                not self.tool_call_sent
                and any(marker.startswith("R3_TEST_MARKER") for marker in markers)
            )
            if should_call_bash:
                self.tool_call_sent = True
            tool_marker = next(
                (marker for marker in markers if marker.startswith("R3_TEST_MARKER")),
                markers[0] if markers else "R3_TEST_MARKER_missing",
            )
            return {
                "index": len(self.requests),
                "markers": markers,
                "tool_marker": tool_marker,
                "should_call_bash": should_call_bash,
            }

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "request_count": len(self.requests),
                "paths": [req["path"] for req in self.requests],
                "tool_call_sent": self.tool_call_sent,
                "requests": self.requests,
            }


def make_handler(state: MockOpenAIProvider):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            payload = json.dumps(
                {"object": "list", "data": [{"id": state.model, "object": "model"}]}
            ).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self) -> None:
            length = int(self.headers.get("Content-Length", "0") or "0")
            body = self.rfile.read(length) if length else b""
            request = state.record(self.path, body)
            if not self.path.endswith("/chat/completions"):
                self.send_response(404)
                self.end_headers()
                return

            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            try:
                if request["should_call_bash"]:
                    _write_bash_tool_call(self.wfile, request["tool_marker"])
                else:
                    text = " ".join(dict.fromkeys(request["markers"])) or "HARNESS_OK"
                    _write_final(self.wfile, text)
            except (BrokenPipeError, ConnectionResetError):
                return
            finally:
                self.close_connection = True

        def log_message(self, _format: str, *_args: Any) -> None:
            return

    return Handler


@contextmanager
def mock_openai_provider(
    model: str = "harness-local-model",
) -> Iterator[tuple[str, MockOpenAIProvider]]:
    state = MockOpenAIProvider(model=model)
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


def apply_mock_provider_env(
    env: dict[str, str],
    base_url: str,
    *,
    model: str,
    name: str,
) -> None:
    env.update(
        {
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_BASE_URL": base_url,
            "MOSSEN_CODE_CUSTOM_MODEL": model,
            "MOSSEN_CODE_CUSTOM_NAME": name,
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-harness-local-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "30",
            "MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS": "30",
        }
    )
