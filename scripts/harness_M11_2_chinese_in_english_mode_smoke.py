#!/usr/bin/env python3
"""
M11.2 - current Rust Chinese input in English mode smoke.

Uses a local OpenAI-compatible mock backend, not a real model. The test proves
that a Chinese prompt is accepted by the current Rust stdin path when the
persisted language preference is English, reaches the provider request body,
produces a non-empty answer, and records a transcript.
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
CHINESE_PROMPT = "你好,请问 1+1 等于几? 请直接给出数字答案, 不需要解释。"
FINAL_MARKER = "M11_2_CHINESE_INPUT_OK"


def iter_strings(value: Any):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from iter_strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from iter_strings(item)


class MockState:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.requests: list[dict[str, Any]] = []
        self.chinese_prompt_seen = False

    def record(self, path: str, body: bytes) -> None:
        try:
            parsed = json.loads(body.decode("utf-8")) if body else {}
        except json.JSONDecodeError:
            parsed = {"_decode_error": body.decode("utf-8", "replace")}
        with self.lock:
            self.requests.append({"path": path, "body": parsed})
            body_text = "\n".join(iter_strings(parsed))
            if "你好" in body_text and "1+1" in body_text:
                self.chinese_prompt_seen = True

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "request_count": len(self.requests),
                "paths": [req["path"] for req in self.requests],
                "chinese_prompt_seen": self.chinese_prompt_seen,
                "requests": self.requests,
            }


def write_sse(wfile, payload: dict[str, Any]) -> None:
    wfile.write(f"data: {json.dumps(payload)}\n\n".encode("utf-8"))
    wfile.flush()


def write_final(wfile) -> None:
    write_sse(
        wfile,
        {
            "id": "m11-2-final",
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {"content": f"{FINAL_MARKER}: 2"}, "finish_reason": None}],
        },
    )
    write_sse(
        wfile,
        {
            "id": "m11-2-final",
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 4},
        },
    )
    wfile.write(b"data: [DONE]\n\n")
    wfile.flush()


def make_handler(state: MockState):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            payload = json.dumps(
                {"object": "list", "data": [{"id": "m11-language-model", "object": "model"}]}
            ).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self) -> None:
            length = int(self.headers.get("Content-Length", "0") or "0")
            body = self.rfile.read(length) if length else b""
            state.record(self.path, body)
            if not self.path.endswith("/chat/completions"):
                self.send_response(404)
                self.end_headers()
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            write_final(self.wfile)

        def log_message(self, _format: str, *_args: Any) -> None:
            return

    return Handler


@contextmanager
def mock_openai_server():
    state = MockState()
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


def main() -> int:
    ctx = make_fixture("M11.2_chinese_input_current_rust")
    env = dict(ctx.env)
    env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    env.update(
        {
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_MODEL": "m11-language-model",
            "MOSSEN_CODE_CUSTOM_NAME": "M11 Language Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-m11-local-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "30",
            "MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS": "30",
        }
    )
    (ctx.mossen_config_home / ".mossen.json").write_text(
        json.dumps({"interactiveLanguagePreference": "en"}),
        encoding="utf-8",
    )
    project = ctx.root_dir / "project_root"
    project.mkdir(parents=True, exist_ok=True)

    with mock_openai_server() as (base_url, model_state):
        env["MOSSEN_CODE_CUSTOM_BASE_URL"] = base_url
        command = [str(ROOT / "scripts" / "start-mossen.sh"), "--stdin", "--cwd", str(project)]
        proc = subprocess.run(
            command,
            input=CHINESE_PROMPT,
            cwd=str(ROOT),
            env=env,
            text=True,
            capture_output=True,
            timeout=120,
        )
        snapshot = model_state.snapshot()

    transcripts = sorted((ctx.home_dir / ".mossen" / "transcripts").glob("*.json"))
    transcript_text = "\n".join(
        path.read_text(encoding="utf-8", errors="replace") for path in transcripts
    )
    requests_path = ctx.artifacts_dir / "model_requests.json"
    requests_path.write_text(json.dumps(snapshot["requests"], indent=2, ensure_ascii=False))
    write_command_log(ctx, command, proc.stdout, proc.stderr, proc.returncode)

    marker_in_stdout = FINAL_MARKER in proc.stdout
    transcript_has_prompt = "你好" in transcript_text and "1+1" in transcript_text
    transcript_has_answer = FINAL_MARKER in transcript_text
    ok = (
        proc.returncode == 0
        and marker_in_stdout
        and snapshot["chinese_prompt_seen"]
        and transcript_has_prompt
        and transcript_has_answer
    )

    result = {
        "name": "chinese_input_in_english_mode_current_rust",
        "ok": ok,
        "exit_code": proc.returncode,
        "marker_in_stdout": marker_in_stdout,
        "chinese_prompt_seen_by_backend": snapshot["chinese_prompt_seen"],
        "request_count": snapshot["request_count"],
        "transcript_count": len(transcripts),
        "transcript_has_prompt": transcript_has_prompt,
        "transcript_has_answer": transcript_has_answer,
        "fixture_root": str(ctx.root_dir),
        "model_requests": str(requests_path),
    }
    write_assertions(
        ctx,
        status="passed" if ok else "failed",
        assertions=[
            {
                "name": result["name"],
                "expected": True,
                "actual": ok,
                "passed": ok,
                "evidence": (
                    f"exit={proc.returncode} backend_prompt={snapshot['chinese_prompt_seen']} "
                    f"stdout_marker={marker_in_stdout} transcript_prompt={transcript_has_prompt}"
                ),
            }
        ],
        extra_artifacts={"model_requests": str(requests_path)},
    )
    print(
        json.dumps(
            {
                "results": [result],
                "passed": 1 if ok else 0,
                "total": 1,
                "fixture_root": str(ctx.root_dir),
                "design_note": "M11.2 validates Chinese input through current Rust stdin/custom-backend/transcript path.",
            },
            indent=2,
            ensure_ascii=False,
        )
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
