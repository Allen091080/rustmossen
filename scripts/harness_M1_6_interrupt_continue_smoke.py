#!/usr/bin/env python3
"""
M1.6 - current Rust SIGTERM cancellation + restore smoke.

This version proves the current personal Rust path:

1. `--stdin` starts a model request against a local OpenAI-compatible mock.
2. SIGTERM while the mock is holding response headers cancels the in-flight
   request quickly instead of waiting for the request timeout.
3. The interrupted oneshot records a transcript, and a fresh `--stdin --restore`
   turn reloads that history without using a real backend.
"""

from __future__ import annotations

import json
import os
import signal
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
P1_PROMPT_MARKER = "INTERRUPT_M1_6_P1_PROMPT_unique_marker_555"
P2_MARKER = "RESTORE_OK_M1_6"


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


def find_transcripts(home_dir: Path) -> list[Path]:
    return sorted((home_dir / ".mossen" / "transcripts").glob("*.json"))


class MockState:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.p1_request_received = threading.Event()
        self.requests: list[dict[str, Any]] = []
        self.phase = "interrupt"
        self.restore_history_seen = False

    def record(self, path: str, body: bytes) -> dict[str, Any]:
        try:
            parsed = json.loads(body.decode("utf-8")) if body else {}
        except json.JSONDecodeError:
            parsed = {"_decode_error": body.decode("utf-8", "replace")}
        with self.lock:
            phase = self.phase
            self.requests.append({"phase": phase, "path": path, "body": parsed})
            request_index = len(self.requests)
            body_text = "\n".join(iter_strings(parsed))
            if phase == "interrupt":
                self.p1_request_received.set()
            if phase == "restore" and P1_PROMPT_MARKER in body_text:
                self.restore_history_seen = True
            return {"index": request_index, "phase": phase, "body_text": body_text}

    def enter_restore_phase(self) -> None:
        with self.lock:
            self.phase = "restore"

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "request_count": len(self.requests),
                "paths": [req["path"] for req in self.requests],
                "restore_history_seen": self.restore_history_seen,
                "phase": self.phase,
                "requests": self.requests,
            }


def write_sse(wfile, payload: dict[str, Any]) -> None:
    wfile.write(f"data: {json.dumps(payload)}\n\n".encode("utf-8"))
    wfile.flush()


def write_text_final(wfile, text: str) -> None:
    write_sse(
        wfile,
        {
            "id": "m1-6-final",
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": None}],
        },
    )
    write_sse(
        wfile,
        {
            "id": "m1-6-final",
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 4},
        },
    )
    wfile.write(b"data: [DONE]\n\n")
    wfile.flush()


class DaemonThreadingHTTPServer(ThreadingHTTPServer):
    daemon_threads = True


def make_handler(state: MockState):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            payload = json.dumps(
                {"object": "list", "data": [{"id": "m1-6-cancel-model", "object": "model"}]}
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

            if request["phase"] == "interrupt":
                time.sleep(60)
                return

            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            try:
                write_text_final(self.wfile, f"{P2_MARKER}: restored interrupted context.")
            except (BrokenPipeError, ConnectionResetError):
                return

        def log_message(self, _format: str, *_args: Any) -> None:
            return

    return Handler


@contextmanager
def mock_openai_server():
    state = MockState()
    server = DaemonThreadingHTTPServer(("127.0.0.1", 0), make_handler(state))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        host, port = server.server_address
        yield f"http://{host}:{port}", state
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def ensure_mossen_built(env: dict[str, str]) -> None:
    proc = subprocess.run(
        ["cargo", "build", "--quiet", "-p", "mossen-cli", "--bin", "mossen"],
        cwd=str(ROOT),
        env=env,
        capture_output=True,
        text=True,
        timeout=180,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            "failed to build mossen test binary\n"
            f"stdout:\n{proc.stdout}\n"
            f"stderr:\n{proc.stderr}"
        )


def case_interrupt_then_restore() -> dict:
    ctx = make_fixture("M1.6_interrupt_continue_current_rust")
    env = dict(ctx.env)
    env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    env.setdefault("MOSSEN_CONFIG_DIR", str(ctx.mossen_config_home))
    env.update(
        {
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_MODEL": "m1-6-cancel-model",
            "MOSSEN_CODE_CUSTOM_NAME": "M1.6 Cancel Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-m1-6-local-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS": "45",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "45",
        }
    )
    ensure_mossen_built(env)
    env["MOSSEN_START_BUILD"] = "never"
    ctx.env.update(env)

    shared_cwd = ctx.root_dir / "project_root"
    shared_cwd.mkdir(parents=True, exist_ok=True)
    p1_prompt = (
        f"标记: {P1_PROMPT_MARKER}. "
        "请开始一个长回复；测试会在服务端返回响应头之前发送 SIGTERM。"
    )
    p2_prompt = f"继续上次中断的会话，并只回答 {P2_MARKER}。"

    with mock_openai_server() as (base_url, model_state):
        env["MOSSEN_CODE_CUSTOM_BASE_URL"] = base_url
        ctx.env.update(env)
        p1_command = [mossen_runner(), "--stdin", "--cwd", str(shared_cwd)]
        p1_proc = subprocess.Popen(
            p1_command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            text=True,
            cwd=str(ROOT),
            start_new_session=True,
        )
        assert p1_proc.stdin is not None
        p1_proc.stdin.write(p1_prompt)
        p1_proc.stdin.close()
        p1_proc.stdin = None

        request_seen = model_state.p1_request_received.wait(timeout=45)
        signal_at = time.monotonic()
        if request_seen:
            try:
                os.killpg(os.getpgid(p1_proc.pid), signal.SIGTERM)
            except (ProcessLookupError, PermissionError):
                pass
        else:
            try:
                os.killpg(os.getpgid(p1_proc.pid), signal.SIGTERM)
            except (ProcessLookupError, PermissionError):
                pass
        try:
            p1_stdout, p1_stderr = p1_proc.communicate(timeout=12)
            p1_timed_out = False
        except subprocess.TimeoutExpired:
            p1_timed_out = True
            try:
                os.killpg(os.getpgid(p1_proc.pid), signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
            p1_stdout, p1_stderr = p1_proc.communicate(timeout=10)
        p1_exit_after_signal_secs = time.monotonic() - signal_at

        p2_command = [mossen_runner(), "--stdin", "--restore", "--cwd", str(shared_cwd)]
        p2_stdout = ""
        p2_stderr = ""
        p2_returncode: int | None = None
        if request_seen:
            model_state.enter_restore_phase()
            try:
                p2 = subprocess.run(
                    p2_command,
                    input=p2_prompt,
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=45,
                    cwd=str(ROOT),
                )
                p2_stdout = p2.stdout
                p2_stderr = p2.stderr
                p2_returncode = p2.returncode
            except subprocess.TimeoutExpired as exc:
                p2_stdout = exc.stdout or ""
                p2_stderr = exc.stderr or ""
                p2_returncode = -999
        server_snapshot = model_state.snapshot()

    transcripts = find_transcripts(ctx.home_dir)
    transcript_text = "\n".join(
        path.read_text(encoding="utf-8", errors="replace") for path in transcripts
    )
    p1_cancelled_prompt_recorded = P1_PROMPT_MARKER in transcript_text
    p2_marker_in_stdout = P2_MARKER in p2_stdout
    restore_history_seen = bool(server_snapshot["restore_history_seen"])
    p1_exited_promptly = (
        request_seen
        and not p1_timed_out
        and p1_exit_after_signal_secs < 8
    )

    stdout = (
        f"=== P1 stdout ===\n{p1_stdout}\n"
        f"=== P2 stdout ===\n{p2_stdout}\n"
        f"=== model requests ===\n"
        f"{json.dumps(server_snapshot, indent=2, ensure_ascii=False)[:4000]}\n"
    )
    stderr = (
        f"=== P1 stderr ===\n{p1_stderr}\n"
        f"=== P2 stderr ===\n{p2_stderr}\n"
        f"=== timing ===\n"
        f"request_seen={request_seen} p1_timed_out={p1_timed_out} "
        f"p1_exit_after_signal_secs={p1_exit_after_signal_secs:.3f}\n"
    )
    write_command_log(
        ctx,
        [*p1_command, "then", *p2_command],
        stdout,
        stderr,
        p2_returncode if p2_returncode is not None else p1_proc.returncode or 1,
    )

    requests_path = ctx.artifacts_dir / "model_requests.json"
    requests_path.write_text(json.dumps(server_snapshot["requests"], indent=2, ensure_ascii=False))
    transcripts_path = ctx.artifacts_dir / "transcripts.txt"
    transcripts_path.write_text(transcript_text, encoding="utf-8")

    ok = (
        p1_exited_promptly
        and p1_cancelled_prompt_recorded
        and p2_returncode == 0
        and p2_marker_in_stdout
        and restore_history_seen
    )
    return {
        "name": "interrupt_then_restore_current_rust",
        "ok": ok,
        "request_seen": request_seen,
        "p1_returncode": p1_proc.returncode,
        "p1_timed_out": p1_timed_out,
        "p1_exit_after_signal_secs": round(p1_exit_after_signal_secs, 3),
        "p1_exited_promptly": p1_exited_promptly,
        "p1_cancelled_prompt_recorded": p1_cancelled_prompt_recorded,
        "p2_exit": p2_returncode,
        "p2_marker_in_stdout": p2_marker_in_stdout,
        "restore_history_seen": restore_history_seen,
        "transcript_count": len(transcripts),
        "fixture_root": str(ctx.root_dir),
        "model_requests": str(requests_path),
        "transcripts": str(transcripts_path),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_interrupt_then_restore()
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
                    f"request_seen={r.get('request_seen')} "
                    f"p1_prompt_exit={r.get('p1_exited_promptly')} "
                    f"p1_secs={r.get('p1_exit_after_signal_secs')} "
                    f"transcript_recorded={r.get('p1_cancelled_prompt_recorded')} "
                    f"p2_exit={r.get('p2_exit')} "
                    f"p2_marker={r.get('p2_marker_in_stdout')} "
                    f"restore_history_seen={r.get('restore_history_seen')}"
                ),
            }
            for r in results
        ],
        extra_artifacts={
            "model_requests": str(ctx.artifacts_dir / "model_requests.json"),
            "transcripts": str(ctx.artifacts_dir / "transcripts.txt"),
        },
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M1.6 validates SIGTERM cancellation is wired through the current "
            "Rust oneshot path and that restore reloads the interrupted transcript."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
