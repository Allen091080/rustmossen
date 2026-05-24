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
HEAD_MARKER = "TERMINAL_HEARTBEAT_HEAD_W279"
TAIL_MARKER = "TERMINAL_HEARTBEAT_TAIL_W279"


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


def make_handler(state: MockState, *, chunks: int, delay_ms: int, initial_delay_ms: int):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            state.record(self.path, dict(self.headers), b"")
            payload = json.dumps(
                {
                    "object": "list",
                    "data": [{"id": "terminal-heartbeat-model", "object": "model"}],
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
                f"terminal-heartbeat-row-{idx:03}: slow first token stayed visible.\n"
                for idx in range(chunks)
            )
            pieces.append(f"{TAIL_MARKER}\n")

            try:
                time.sleep(initial_delay_ms / 1000.0)
                for piece in pieces:
                    payload = {
                        "id": "terminal-heartbeat",
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
                    "id": "terminal-heartbeat",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 12, "completion_tokens": chunks + 2},
                }
                self.wfile.write(f"data: {json.dumps(final_payload)}\n\n".encode("utf-8"))
                self.wfile.write(b"data: [DONE]\n\n")
                self.wfile.flush()
                state.mark_completed()
            except (BrokenPipeError, ConnectionResetError):
                return
            finally:
                self.close_connection = True

        def log_message(self, _format: str, *_args: Any) -> None:
            return

    return Handler


def start_mock_server(
    chunks: int, delay_ms: int, initial_delay_ms: int
) -> tuple[HTTPServer, MockState, threading.Thread]:
    state = MockState()
    server = HTTPServer(
        ("127.0.0.1", free_port()),
        make_handler(
            state,
            chunks=chunks,
            delay_ms=delay_ms,
            initial_delay_ms=initial_delay_ms,
        ),
    )
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


def decode_output(output: bytes | bytearray) -> str:
    return bytes(output).decode("utf-8", errors="replace")


def run_slow_first_token_heartbeat_smoke() -> dict[str, Any]:
    if not RUN_MOSSEN.exists():
        raise RuntimeError(f"missing launcher: {RUN_MOSSEN}")

    ctx = make_fixture("W279_terminal_slow_first_token_heartbeat_pty_smoke")
    project = ctx.root_dir / "project"
    project.mkdir(parents=True, exist_ok=True)

    chunks = int(os.environ.get("MOSSEN_TERMINAL_HEARTBEAT_PTY_CHUNKS", "12"))
    delay_ms = int(os.environ.get("MOSSEN_TERMINAL_HEARTBEAT_PTY_DELAY_MS", "3"))
    initial_delay_ms = int(os.environ.get("MOSSEN_TERMINAL_HEARTBEAT_PTY_INITIAL_DELAY_MS", "1500"))
    server, mock_state, thread = start_mock_server(chunks, delay_ms, initial_delay_ms)
    port = server.server_address[1]
    base_url = f"http://127.0.0.1:{port}/v1"
    diagnostics_path = ctx.artifacts_dir / "terminal_render_diagnostics.json"

    env = ctx.env.copy()
    env.update(
        {
            "MOSSEN_CONFIG_DIR": str(ctx.mossen_config_home),
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_BASE_URL": base_url,
            "MOSSEN_CODE_CUSTOM_MODEL": "terminal-heartbeat-model",
            "MOSSEN_CODE_CUSTOM_NAME": "Terminal Heartbeat Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-terminal-heartbeat-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_DISABLE_THINKING": "1",
            "MOSSEN_CODE_DISABLE_ADAPTIVE_THINKING": "1",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "30",
            "MOSSEN_TERMINAL_RENDER_DIAGNOSTICS_PATH": str(diagnostics_path),
            "TERM": "xterm-256color",
            "TERM_PROGRAM": "WezTerm",
        }
    )
    for key in list(env):
        if key.startswith("PROVIDER_"):
            del env[key]

    real_home = Path(os.environ.get("HOME", str(Path.home())))
    cargo_home = Path(os.environ.get("CARGO_HOME", str(real_home / ".cargo")))
    rustup_home = Path(os.environ.get("RUSTUP_HOME", str(real_home / ".rustup")))
    if cargo_home.exists():
        env["CARGO_HOME"] = str(cargo_home)
    if rustup_home.exists():
        env["RUSTUP_HOME"] = str(rustup_home)

    skip_build = os.environ.get("MOSSEN_TERMINAL_HEARTBEAT_PTY_SKIP_BUILD") == "1"
    if not skip_build:
        build_timeout = float(os.environ.get("MOSSEN_TERMINAL_HEARTBEAT_PTY_BUILD_TIMEOUT_SECS", "300"))
        build_proc = subprocess.run(
            ["cargo", "build", "--quiet", "-p", "mossen-cli", "--bin", "mossen"],
            cwd=str(ROOT),
            env=env,
            capture_output=True,
            text=True,
            timeout=build_timeout,
        )
        if build_proc.returncode != 0:
            (ctx.artifacts_dir / "build_stdout.txt").write_text(build_proc.stdout)
            (ctx.artifacts_dir / "build_stderr.txt").write_text(build_proc.stderr)
            raise RuntimeError(
                "failed to build mossen-cli for W279 terminal heartbeat PTY smoke; "
                f"see {ctx.artifacts_dir / 'build_stderr.txt'}"
            )
    elif not (DEBUG_MOSSEN.exists() and os.access(DEBUG_MOSSEN, os.X_OK)):
        raise RuntimeError(f"missing debug binary with build skipped: {DEBUG_MOSSEN}")

    command = [
        str(DEBUG_MOSSEN),
        "--bare",
        "--oneshot",
        "Stream terminal heartbeat markers after a slow first token.",
        "--emit",
        "terminal",
    ]

    master_fd, slave_fd = pty.openpty()
    set_pty_size(slave_fd, rows=24, cols=96)
    output = bytearray()
    proc: subprocess.Popen[bytes] | None = None
    started = time.time()
    timeout = float(os.environ.get("MOSSEN_TERMINAL_HEARTBEAT_PTY_TIMEOUT_SECS", "90"))

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

        while time.time() - started < timeout:
            read_pty(master_fd, output, timeout=0.05)
            if proc.poll() is not None:
                break

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

    text = decode_output(output)
    raw_path = ctx.artifacts_dir / "pty_raw_output.bin"
    text_path = ctx.artifacts_dir / "pty_output.txt"
    mock_path = ctx.artifacts_dir / "mock_requests.json"
    raw_path.write_bytes(bytes(output))
    text_path.write_text(text, encoding="utf-8", errors="replace")
    mock_path.write_text(json.dumps(mock_state.snapshot(), indent=2, ensure_ascii=False))

    diagnostics: dict[str, Any] = {}
    diagnostics_parse_error = ""
    if diagnostics_path.exists():
        try:
            diagnostics = json.loads(diagnostics_path.read_text(encoding="utf-8"))
        except Exception as exc:
            diagnostics_parse_error = str(exc)

    exit_code = proc.returncode if proc is not None and proc.returncode is not None else -1
    write_command_log(ctx, command, text, "", exit_code)

    snapshot = mock_state.snapshot()
    chat_hit = any(req.get("path", "").endswith("/chat/completions") for req in snapshot["requests"])
    head_pos = text.find(HEAD_MARKER)
    pre_head = text[:head_pos] if head_pos >= 0 else text
    heartbeat_before_head = "waiting for model stream" in pre_head
    elapsed_before_head = "Thinking 1s" in pre_head
    alt_enters = text.count("\x1b[?1049h")
    alt_leaves = text.count("\x1b[?1049l")
    sync_enters = text.count("\x1b[?2026h")
    sync_leaves = text.count("\x1b[?2026l")
    bracketed_enters = text.count("\x1b[?2004h")
    bracketed_leaves = text.count("\x1b[?2004l")
    full_clears = text.count("\x1b[2J") + text.count("\x1b[3J")
    last_report = diagnostics.get("lastReport") or {}
    execution = last_report.get("execution") or {}
    assertions = [
        ("process_exited_zero", exit_code == 0, f"exit={exit_code}"),
        ("mock_chat_completion_hit", chat_hit, f"requests={len(snapshot['requests'])}"),
        ("mock_streamed_chunks", snapshot["chunks_sent"] >= chunks, f"chunks={snapshot['chunks_sent']}"),
        ("mock_completed_stream", snapshot["completed"], f"chunks={snapshot['chunks_sent']}"),
        ("head_marker_rendered", head_pos >= 0, HEAD_MARKER),
        ("tail_marker_rendered", TAIL_MARKER in text, TAIL_MARKER),
        ("heartbeat_status_before_first_token", heartbeat_before_head, pre_head[-500:]),
        ("elapsed_heartbeat_before_first_token", elapsed_before_head, pre_head[-500:]),
        ("diagnostics_file_written", diagnostics_path.exists(), str(diagnostics_path)),
        ("diagnostics_json_parseable", bool(diagnostics) and not diagnostics_parse_error, diagnostics_parse_error),
        ("diagnostics_no_pending_draw", diagnostics.get("hasPendingDraw") is False, str(diagnostics.get("hasPendingDraw"))),
        ("diagnostics_manual_scroll_released", diagnostics.get("manualScrollActive") is False, str(diagnostics.get("manualScrollActive"))),
        ("diagnostics_reports_recorded", diagnostics.get("reportCount", 0) > 0, str(diagnostics.get("reportCount"))),
        ("diagnostics_applied_draws", diagnostics.get("appliedReportCount", 0) > 0, str(diagnostics.get("appliedReportCount"))),
        ("diagnostics_last_report_applied", last_report.get("applied") is True, str(last_report)),
        ("diagnostics_last_execution_flushed", execution.get("flushed") is True, str(execution)),
        ("diagnostics_no_terminal_op_budget_overflow", execution.get("terminalOpBudgetExceeded") is False, str(execution)),
        ("alt_screen_balanced", alt_enters == alt_leaves, f"alt_enters={alt_enters} alt_leaves={alt_leaves}"),
        ("sync_update_used", sync_enters > 0, f"sync_enters={sync_enters}"),
        ("sync_update_balanced", sync_enters == sync_leaves, f"sync_enters={sync_enters} sync_leaves={sync_leaves}"),
        ("bracketed_paste_balanced", bracketed_enters == bracketed_leaves, f"bracketed={bracketed_enters}/{bracketed_leaves}"),
        ("no_repeated_fullscreen_clear", full_clears == 0, f"full_clears={full_clears}"),
        ("output_size_bounded", len(output) < 2_500_000, f"bytes={len(output)}"),
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
            "diagnostics": str(diagnostics_path),
        },
    )

    return {
        "ok": all(passed for _, passed, _ in assertions),
        "fixture_root": str(ctx.root_dir),
        "exit_code": exit_code,
        "mock": {
            "request_count": len(snapshot["requests"]),
            "chunks_sent": snapshot["chunks_sent"],
            "completed": snapshot["completed"],
        },
        "heartbeat_before_head": heartbeat_before_head,
        "elapsed_before_head": elapsed_before_head,
        "head_marker": head_pos >= 0,
        "tail_marker": TAIL_MARKER in text,
        "diagnostics": {
            "path": str(diagnostics_path),
            "hasPendingDraw": diagnostics.get("hasPendingDraw"),
            "manualScrollActive": diagnostics.get("manualScrollActive"),
            "reportCount": diagnostics.get("reportCount"),
            "appliedReportCount": diagnostics.get("appliedReportCount"),
            "queuedReportCount": diagnostics.get("queuedReportCount"),
            "droppedPendingCount": diagnostics.get("droppedPendingCount"),
            "lastReportApplied": last_report.get("applied"),
            "lastExecutionFlushed": execution.get("flushed"),
        },
        "alt_enters": alt_enters,
        "alt_leaves": alt_leaves,
        "sync_enters": sync_enters,
        "sync_leaves": sync_leaves,
        "bracketed_enters": bracketed_enters,
        "bracketed_leaves": bracketed_leaves,
        "full_clears": full_clears,
        "output_bytes": len(output),
        "artifacts": {
            "pty_output": str(text_path),
            "mock_requests": str(mock_path),
            "diagnostics": str(diagnostics_path),
            "assertions": str(ctx.artifacts_dir / "assertions.json"),
        },
    }


def main() -> int:
    result = run_slow_first_token_heartbeat_smoke()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
