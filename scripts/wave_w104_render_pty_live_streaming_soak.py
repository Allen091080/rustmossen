#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import pty
import select
import signal
import socket
import struct
import subprocess
import sys
import termios
import threading
import time
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_MOSSEN = ROOT / "scripts" / "start-mossen.sh"
DEBUG_MOSSEN = ROOT / "target" / "debug" / "mossen"
HEAD_MARKER = "PTY_SOAK_HEAD_W104"
TAIL_MARKER = "PTY_SOAK_TAIL_W104"


@dataclass
class MockState:
    requests: list[dict[str, Any]] = field(default_factory=list)
    chunks_sent: int = 0
    completed: bool = False
    lock: threading.Lock = field(default_factory=threading.Lock)

    def record(self, path: str, headers: dict[str, str], body: bytes) -> None:
        with self.lock:
            self.requests.append(
                {
                    "path": path,
                    "authorization": headers.get("Authorization", ""),
                    "x_api_key": headers.get("x-api-key", ""),
                    "body": body.decode("utf-8", errors="replace")[:4000],
                    "ts": time.time(),
                }
            )

    def mark_chunk(self) -> None:
        with self.lock:
            self.chunks_sent += 1

    def mark_completed(self) -> None:
        with self.lock:
            self.completed = True

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "requests": list(self.requests),
                "chunks_sent": self.chunks_sent,
                "completed": self.completed,
            }


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def make_handler(state: MockState, *, chunks: int, delay_ms: int):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            state.record(self.path, dict(self.headers), b"")
            payload = json.dumps(
                {
                    "object": "list",
                    "data": [{"id": "pty-soak-model", "object": "model"}],
                }
            ).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self) -> None:
            length = int(self.headers.get("Content-Length", "0") or "0")
            body = self.rfile.read(length) if length else b""
            state.record(self.path, dict(self.headers), body)
            if not self.path.endswith("/chat/completions"):
                self.send_response(404)
                self.end_headers()
                return

            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()

            pieces = [f"{HEAD_MARKER}\n"]
            pieces.extend(
                f"stream-row-{idx:04}: PTY live streaming renderer must stay paced and scroll-safe.\n"
                for idx in range(chunks)
            )
            pieces.append(f"{TAIL_MARKER}\n")

            for piece in pieces:
                payload = {
                    "id": "pty-soak",
                    "object": "chat.completion.chunk",
                    "choices": [
                        {
                            "index": 0,
                            "delta": {"content": piece},
                            "finish_reason": None,
                        }
                    ],
                }
                self.wfile.write(f"data: {json.dumps(payload)}\n\n".encode("utf-8"))
                self.wfile.flush()
                state.mark_chunk()
                time.sleep(delay_ms / 1000.0)

            final_payload = {
                "id": "pty-soak",
                "object": "chat.completion.chunk",
                "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 12, "completion_tokens": chunks + 2},
            }
            self.wfile.write(f"data: {json.dumps(final_payload)}\n\n".encode("utf-8"))
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            state.mark_completed()
            self.close_connection = True

        def log_message(self, _format: str, *_args: Any) -> None:
            return

    return Handler


def start_mock_server(chunks: int, delay_ms: int) -> tuple[HTTPServer, MockState, threading.Thread]:
    state = MockState()
    server = HTTPServer(("127.0.0.1", free_port()), make_handler(state, chunks=chunks, delay_ms=delay_ms))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, state, thread


def set_pty_size(fd: int, *, rows: int, cols: int) -> None:
    winsz = struct.pack("HHHH", rows, cols, 0, 0)
    if hasattr(termios, "tcsetwinsize"):
        try:
            termios.tcsetwinsize(fd, (rows, cols))
        except Exception:
            pass
    try:
        import fcntl

        fcntl.ioctl(fd, termios.TIOCSWINSZ, winsz)
    except Exception:
        pass


def read_pty(master_fd: int, output: bytearray, *, timeout: float) -> bool:
    readable, _, _ = select.select([master_fd], [], [], timeout)
    if not readable:
        return False
    try:
        chunk = os.read(master_fd, 8192)
    except OSError:
        return False
    if chunk:
        output.extend(chunk)
        return True
    return False


def decode_output(output: bytes) -> str:
    return output.decode("utf-8", errors="replace")


def run_pty_soak() -> dict[str, Any]:
    if not RUN_MOSSEN.exists():
        raise RuntimeError(f"missing launcher: {RUN_MOSSEN}")

    ctx = make_fixture("W104_render_pty_live_streaming_soak")
    project = ctx.root_dir / "project"
    project.mkdir(parents=True, exist_ok=True)

    chunks = int(os.environ.get("MOSSEN_PTY_SOAK_CHUNKS", "96"))
    delay_ms = int(os.environ.get("MOSSEN_PTY_SOAK_DELAY_MS", "12"))
    server, mock_state, thread = start_mock_server(chunks, delay_ms)
    port = server.server_address[1]
    base_url = f"http://127.0.0.1:{port}/v1"

    env = ctx.env.copy()
    env.update(
        {
            "MOSSEN_CONFIG_DIR": str(ctx.mossen_config_home),
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_BASE_URL": base_url,
            "MOSSEN_CODE_CUSTOM_MODEL": "pty-soak-model",
            "MOSSEN_CODE_CUSTOM_NAME": "PTY Soak Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-pty-soak-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_DISABLE_THINKING": "1",
            "MOSSEN_CODE_DISABLE_ADAPTIVE_THINKING": "1",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "30",
            "TERM": "xterm-256color",
            "TERM_PROGRAM": "WezTerm",
        }
    )
    for key in list(env):
        if key.startswith("PROVIDER_"):
            del env[key]

    master_fd, slave_fd = pty.openpty()
    set_pty_size(slave_fd, rows=24, cols=96)
    real_home = Path(os.environ.get("HOME", str(Path.home())))
    cargo_home = Path(os.environ.get("CARGO_HOME", str(real_home / ".cargo")))
    rustup_home = Path(os.environ.get("RUSTUP_HOME", str(real_home / ".rustup")))
    if cargo_home.exists():
        env["CARGO_HOME"] = str(cargo_home)
    if rustup_home.exists():
        env["RUSTUP_HOME"] = str(rustup_home)

    force_build = os.environ.get("MOSSEN_PTY_SOAK_FORCE_BUILD") == "1"
    if DEBUG_MOSSEN.exists() and os.access(DEBUG_MOSSEN, os.X_OK) and not force_build:
        command = [str(DEBUG_MOSSEN), "--bare"]
    else:
        command = [str(RUN_MOSSEN), "--bare"]
    output = bytearray()
    proc: subprocess.Popen[bytes] | None = None
    sent_prompt = False
    sent_quit = False
    started = time.time()
    timeout = float(os.environ.get("MOSSEN_PTY_SOAK_TIMEOUT_SECS", "120"))

    try:
        proc = subprocess.Popen(
            command,
            cwd=str(project),
            env=env,
            stdin=slave_fd,
            stdout=slave_fd,
            stderr=slave_fd,
            close_fds=True,
        )
        os.close(slave_fd)
        prompt = (
            "请输出一个长 streaming 回复，包含 PTY soak head/tail markers，"
            "不要调用工具。"
        )

        while time.time() - started < timeout:
            read_pty(master_fd, output, timeout=0.05)
            text = decode_output(output)

            if not sent_prompt and ("\x1b[?1049h" in text or "send" in text or "Mossen" in text):
                os.write(master_fd, (prompt + "\r").encode("utf-8"))
                sent_prompt = True

            if sent_prompt and not sent_quit and TAIL_MARKER in text and mock_state.snapshot()["completed"]:
                os.write(master_fd, b"/quit\r")
                sent_quit = True

            if sent_quit and proc.poll() is not None:
                break

            if proc.poll() is not None and sent_prompt:
                break

        if proc.poll() is None:
            if sent_prompt and not sent_quit:
                try:
                    os.write(master_fd, b"/quit\r")
                    sent_quit = True
                    for _ in range(80):
                        read_pty(master_fd, output, timeout=0.05)
                        if proc.poll() is not None:
                            break
                except OSError:
                    pass
            if proc.poll() is None:
                proc.send_signal(signal.SIGINT)
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)

        for _ in range(20):
            if not read_pty(master_fd, output, timeout=0.02):
                break
    finally:
        try:
            os.close(master_fd)
        except OSError:
            pass
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)

    text = decode_output(bytes(output))
    raw_path = ctx.artifacts_dir / "pty_raw_output.bin"
    text_path = ctx.artifacts_dir / "pty_output.txt"
    mock_path = ctx.artifacts_dir / "mock_requests.json"
    raw_path.write_bytes(bytes(output))
    text_path.write_text(text, encoding="utf-8", errors="replace")
    mock_path.write_text(json.dumps(mock_state.snapshot(), indent=2, ensure_ascii=False))

    exit_code = proc.returncode if proc is not None and proc.returncode is not None else -1
    write_command_log(ctx, command, text, "", exit_code)

    snapshot = mock_state.snapshot()
    chat_hit = any(req.get("path", "").endswith("/chat/completions") for req in snapshot["requests"])
    streamed_multiple_chunks = snapshot["chunks_sent"] > 2
    alt_enters = text.count("\x1b[?1049h")
    alt_leaves = text.count("\x1b[?1049l")
    full_clears = text.count("\x1b[2J") + text.count("\x1b[3J")
    output_bytes = len(output)

    assertions = [
        ("process_exited_zero", exit_code == 0, f"exit={exit_code}"),
        ("prompt_sent", sent_prompt, f"sent_prompt={sent_prompt}"),
        ("mock_chat_completion_hit", chat_hit, f"requests={len(snapshot['requests'])}"),
        ("mock_streamed_multiple_chunks", streamed_multiple_chunks, f"chunks={snapshot['chunks_sent']}"),
        ("mock_completed_stream", snapshot["completed"], f"chunks={snapshot['chunks_sent']}"),
        ("head_marker_rendered", HEAD_MARKER in text, HEAD_MARKER),
        ("tail_marker_rendered", TAIL_MARKER in text, TAIL_MARKER),
        ("entered_alt_screen_once", alt_enters == 1, f"alt_enters={alt_enters}"),
        ("left_alt_screen_once", alt_leaves == 1, f"alt_leaves={alt_leaves}"),
        ("no_repeated_fullscreen_clear", full_clears == 0, f"full_clears={full_clears}"),
        ("output_size_bounded", output_bytes < 2_500_000, f"bytes={output_bytes}"),
    ]

    write_assertions(
        ctx,
        status="passed" if all(passed for _, passed, _ in assertions) else "failed",
        assertions=[
            {
                "name": name,
                "expected": True,
                "actual": passed,
                "passed": passed,
                "evidence": evidence,
            }
            for name, passed, evidence in assertions
        ],
        extra_artifacts={
            "pty_raw_output": str(raw_path),
            "pty_output": str(text_path),
            "mock_requests": str(mock_path),
        },
    )

    return {
        "ok": all(passed for _, passed, _ in assertions),
        "fixture_root": str(ctx.root_dir),
        "exit_code": exit_code,
        "sent_prompt": sent_prompt,
        "sent_quit": sent_quit,
        "mock": snapshot,
        "alt_enters": alt_enters,
        "alt_leaves": alt_leaves,
        "full_clears": full_clears,
        "output_bytes": output_bytes,
        "head_marker": HEAD_MARKER in text,
        "tail_marker": TAIL_MARKER in text,
        "artifacts": {
            "pty_output": str(text_path),
            "mock_requests": str(mock_path),
            "assertions": str(ctx.artifacts_dir / "assertions.json"),
        },
    }


def main() -> int:
    result = run_pty_soak()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
