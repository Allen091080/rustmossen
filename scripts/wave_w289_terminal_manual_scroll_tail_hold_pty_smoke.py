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

DEBUG_MOSSEN = ROOT / "target" / "debug" / "mossen"
HEAD_MARKER = "TERMINAL_MANUAL_SCROLL_TAIL_HOLD_HEAD_W289"
TAIL_MARKER = "TERMINAL_MANUAL_SCROLL_TAIL_HOLD_TAIL_W289"


@dataclass
class MockState:
    requests: list[dict[str, Any]] = field(default_factory=list)
    chunks_sent: int = 0
    content_completed: bool = False
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

    def mark_content_completed(self) -> None:
        with self.lock:
            self.content_completed = True

    def mark_completed(self) -> None:
        with self.lock:
            self.completed = True

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "requests": list(self.requests),
                "chunks_sent": self.chunks_sent,
                "content_completed": self.content_completed,
                "completed": self.completed,
            }


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def make_handler(state: MockState, *, chunks: int, delay_ms: int, tail_pause_ms: int):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            state.record(self.path, dict(self.headers), b"")
            payload = json.dumps(
                {
                    "object": "list",
                    "data": [{"id": "terminal-manual-scroll-tail-hold-model", "object": "model"}],
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
                f"tail-hold-row-{idx:03}: manual scroll must keep late stream updates out of the visible terminal.\n"
                for idx in range(chunks)
            )
            pieces.append(f"{TAIL_MARKER}\n")

            try:
                for piece in pieces:
                    payload = {
                        "id": "terminal-manual-scroll-tail-hold",
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

                state.mark_content_completed()
                time.sleep(tail_pause_ms / 1000.0)

                final_payload = {
                    "id": "terminal-manual-scroll-tail-hold",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": chunks + 2},
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
    chunks: int, delay_ms: int, tail_pause_ms: int
) -> tuple[HTTPServer, MockState, threading.Thread]:
    state = MockState()
    server = HTTPServer(
        ("127.0.0.1", free_port()),
        make_handler(state, chunks=chunks, delay_ms=delay_ms, tail_pause_ms=tail_pause_ms),
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


def resize_child(master_fd: int, proc: subprocess.Popen[bytes], *, rows: int, cols: int) -> None:
    set_pty_size(master_fd, rows=rows, cols=cols)
    try:
        os.kill(proc.pid, signal.SIGWINCH)
    except ProcessLookupError:
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


def send_key(master_fd: int, payload: bytes, *, repeat: int = 1, delay: float = 0.02) -> None:
    for _ in range(repeat):
        os.write(master_fd, payload)
        if delay:
            time.sleep(delay)


def run_manual_scroll_tail_hold_smoke() -> dict[str, Any]:
    ctx = make_fixture(
        os.environ.get(
            "MOSSEN_TERMINAL_TAIL_HOLD_PTY_FIXTURE_NAME",
            "W289_terminal_manual_scroll_tail_hold_pty_smoke",
        )
    )
    project = ctx.root_dir / "project"
    project.mkdir(parents=True, exist_ok=True)

    chunks = int(os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_CHUNKS", "96"))
    delay_ms = int(os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_DELAY_MS", "5"))
    tail_pause_ms = int(os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_TAIL_PAUSE_MS", "900"))
    restore_after_content = (
        os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_RESTORE_AFTER_CONTENT", "1") != "0"
    )
    resize_during_hold = os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_RESIZE_DURING_HOLD", "0") != "0"
    resize_narrow_rows = int(os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_NARROW_ROWS", "18"))
    resize_narrow_cols = int(os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_NARROW_COLS", "64"))
    resize_wide_rows = int(os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_FINAL_ROWS", "28"))
    resize_wide_cols = int(os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_FINAL_COLS", "118"))
    server, mock_state, thread = start_mock_server(chunks, delay_ms, tail_pause_ms)
    port = server.server_address[1]
    base_url = f"http://127.0.0.1:{port}/v1"
    diagnostics_path = ctx.artifacts_dir / "terminal_render_diagnostics.json"

    env = ctx.env.copy()
    env.update(
        {
            "MOSSEN_CONFIG_DIR": str(ctx.mossen_config_home),
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_BASE_URL": base_url,
            "MOSSEN_CODE_CUSTOM_MODEL": "terminal-manual-scroll-tail-hold-model",
            "MOSSEN_CODE_CUSTOM_NAME": "Terminal Manual Scroll Tail Hold Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-terminal-tail-hold-fake",
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

    skip_build = os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_SKIP_BUILD") == "1"
    if not skip_build:
        build_timeout = float(os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_BUILD_TIMEOUT_SECS", "300"))
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
                "failed to build mossen-cli for W289 terminal manual-scroll tail hold PTY smoke; "
                f"see {ctx.artifacts_dir / 'build_stderr.txt'}"
            )
    elif not (DEBUG_MOSSEN.exists() and os.access(DEBUG_MOSSEN, os.X_OK)):
        raise RuntimeError(f"missing debug binary with build skipped: {DEBUG_MOSSEN}")

    command = [
        str(DEBUG_MOSSEN),
        "--bare",
        "--oneshot",
        "Stream terminal manual-scroll tail-hold diagnostics markers without calling tools.",
        "--emit",
        "terminal",
    ]

    master_fd, slave_fd = pty.openpty()
    set_pty_size(slave_fd, rows=24, cols=96)
    output = bytearray()
    actions: list[dict[str, Any]] = []
    proc: subprocess.Popen[bytes] | None = None
    started = time.time()
    timeout = float(os.environ.get("MOSSEN_TERMINAL_TAIL_HOLD_PTY_TIMEOUT_SECS", "100"))
    sent_page_up = False
    sent_narrow_resize = False
    sent_wide_resize = False
    sent_restore = False
    page_up_offset: int | None = None
    narrow_resize_offset: int | None = None
    wide_resize_offset: int | None = None
    content_complete_offset: int | None = None
    restore_offset: int | None = None

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
            read_pty(master_fd, output, timeout=0.04)
            snapshot = mock_state.snapshot()
            chunks_sent = snapshot["chunks_sent"]

            if not sent_page_up and chunks_sent >= 16 and proc.poll() is None:
                send_key(master_fd, b"\x1b[5~", repeat=6)
                sent_page_up = True
                page_up_offset = len(output)
                actions.append(
                    {
                        "name": "page_up_manual_scroll",
                        "chunks_sent": chunks_sent,
                        "offset": page_up_offset,
                    }
                )

            if (
                resize_during_hold
                and sent_page_up
                and not sent_narrow_resize
                and chunks_sent >= 36
                and proc.poll() is None
            ):
                resize_child(master_fd, proc, rows=resize_narrow_rows, cols=resize_narrow_cols)
                sent_narrow_resize = True
                narrow_resize_offset = len(output)
                actions.append(
                    {
                        "name": "resize_narrow_while_manual_scroll",
                        "rows": resize_narrow_rows,
                        "cols": resize_narrow_cols,
                        "chunks_sent": chunks_sent,
                        "offset": narrow_resize_offset,
                    }
                )

            if (
                resize_during_hold
                and sent_narrow_resize
                and not sent_wide_resize
                and chunks_sent >= 72
                and proc.poll() is None
            ):
                resize_child(master_fd, proc, rows=resize_wide_rows, cols=resize_wide_cols)
                sent_wide_resize = True
                wide_resize_offset = len(output)
                actions.append(
                    {
                        "name": "resize_wide_while_manual_scroll",
                        "rows": resize_wide_rows,
                        "cols": resize_wide_cols,
                        "chunks_sent": chunks_sent,
                        "offset": wide_resize_offset,
                    }
                )

            if (
                sent_page_up
                and content_complete_offset is None
                and snapshot["content_completed"]
            ):
                content_complete_offset = len(output)
                actions.append(
                    {
                        "name": "content_complete_while_manual_scroll",
                        "chunks_sent": chunks_sent,
                        "offset": content_complete_offset,
                    }
                )

            if (
                restore_after_content
                and content_complete_offset is not None
                and not sent_restore
                and proc.poll() is None
            ):
                send_key(master_fd, b"\x0c", repeat=1)
                sent_restore = True
                restore_offset = len(output)
                actions.append(
                    {
                        "name": "ctrl_l_restore_after_tail_hold",
                        "chunks_sent": chunks_sent,
                        "offset": restore_offset,
                    }
                )

            if proc.poll() is not None:
                break

        if proc.poll() is None:
            if restore_after_content and sent_page_up and not sent_restore:
                send_key(master_fd, b"\x0c", repeat=1)
                sent_restore = True
                restore_offset = len(output)
                actions.append(
                    {
                        "name": "ctrl_l_restore_after_tail_hold_fallback",
                        "chunks_sent": mock_state.snapshot()["chunks_sent"],
                        "offset": restore_offset,
                    }
                )
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
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
    actions_path = ctx.artifacts_dir / "actions.json"
    raw_path.write_bytes(bytes(output))
    text_path.write_text(text, encoding="utf-8", errors="replace")
    mock_path.write_text(json.dumps(mock_state.snapshot(), indent=2, ensure_ascii=False))
    actions_path.write_text(json.dumps(actions, indent=2, ensure_ascii=False))

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
    alt_enters = text.count("\x1b[?1049h")
    alt_leaves = text.count("\x1b[?1049l")
    sync_enters = text.count("\x1b[?2026h")
    sync_leaves = text.count("\x1b[?2026l")
    bracketed_enters = text.count("\x1b[?2004h")
    bracketed_leaves = text.count("\x1b[?2004l")
    full_clears = text.count("\x1b[2J") + text.count("\x1b[3J")
    last_report = diagnostics.get("lastReport") or {}
    execution = last_report.get("execution") or {}
    manual_hold_count = diagnostics.get("manualScrollPreservedReportCount", 0)
    teardown_release_count = diagnostics.get("manualScrollTeardownReleaseCount", 0)
    viewport_columns = execution.get("viewportColumns")
    tail_hold_growth = None
    if page_up_offset is not None and content_complete_offset is not None:
        tail_hold_growth = content_complete_offset - page_up_offset
    narrow_resize_growth = None
    if page_up_offset is not None and narrow_resize_offset is not None:
        narrow_resize_growth = narrow_resize_offset - page_up_offset
    wide_resize_growth = None
    if page_up_offset is not None and wide_resize_offset is not None:
        wide_resize_growth = wide_resize_offset - page_up_offset
    restore_growth = None
    if restore_offset is not None:
        restore_growth = len(output) - restore_offset
    post_content_growth = None
    if content_complete_offset is not None:
        post_content_growth = len(output) - content_complete_offset

    assertions = [
        ("process_exited_zero", exit_code == 0, f"exit={exit_code}"),
        ("mock_chat_completion_hit", chat_hit, f"requests={len(snapshot['requests'])}"),
        ("mock_streamed_chunks", snapshot["chunks_sent"] >= chunks + 2, f"chunks={snapshot['chunks_sent']}"),
        ("mock_content_completed", snapshot["content_completed"], str(snapshot)),
        ("mock_completed_stream", snapshot["completed"], str(snapshot)),
        ("page_up_sent", sent_page_up, str(actions)),
        ("content_complete_seen_before_restore", content_complete_offset is not None, str(actions)),
        (
            "restore_policy_observed",
            sent_restore if restore_after_content else not sent_restore,
            f"restore_after_content={restore_after_content} actions={actions}",
        ),
        (
            "resize_policy_observed",
            (
                sent_narrow_resize and sent_wide_resize
                if resize_during_hold
                else not sent_narrow_resize and not sent_wide_resize
            ),
            f"resize_during_hold={resize_during_hold} actions={actions}",
        ),
        (
            "narrow_resize_output_held",
            not resize_during_hold or (narrow_resize_growth is not None and narrow_resize_growth <= 1024),
            f"narrow_resize_growth={narrow_resize_growth} page_up_offset={page_up_offset} narrow_resize_offset={narrow_resize_offset}",
        ),
        (
            "wide_resize_output_held",
            not resize_during_hold or (wide_resize_growth is not None and wide_resize_growth <= 1024),
            f"wide_resize_growth={wide_resize_growth} page_up_offset={page_up_offset} wide_resize_offset={wide_resize_offset}",
        ),
        (
            "tail_output_held_during_manual_scroll",
            tail_hold_growth is not None and tail_hold_growth <= 1024,
            f"tail_hold_growth={tail_hold_growth} page_up_offset={page_up_offset} content_complete_offset={content_complete_offset}",
        ),
        (
            "output_grew_after_release",
            (
                restore_growth is not None and restore_growth > 256
                if restore_after_content
                else post_content_growth is not None and post_content_growth > 256
            ),
            f"restore_growth={restore_growth} post_content_growth={post_content_growth} restore_offset={restore_offset} output_bytes={len(output)}",
        ),
        (
            "teardown_release_policy_observed",
            teardown_release_count == 0 if restore_after_content else teardown_release_count > 0,
            f"restore_after_content={restore_after_content} teardown_release_count={teardown_release_count}",
        ),
        ("head_marker_rendered", HEAD_MARKER in text, HEAD_MARKER),
        ("tail_marker_rendered", TAIL_MARKER in text, TAIL_MARKER),
        ("diagnostics_file_written", diagnostics_path.exists(), str(diagnostics_path)),
        ("diagnostics_json_parseable", bool(diagnostics) and not diagnostics_parse_error, diagnostics_parse_error),
        ("diagnostics_no_pending_draw", diagnostics.get("hasPendingDraw") is False, str(diagnostics.get("hasPendingDraw"))),
        ("diagnostics_manual_scroll_released", diagnostics.get("manualScrollActive") is False, str(diagnostics.get("manualScrollActive"))),
        ("diagnostics_manual_scroll_preserved_reports", manual_hold_count > 0, str(manual_hold_count)),
        ("diagnostics_queued_reports", diagnostics.get("queuedReportCount", 0) >= manual_hold_count, str(diagnostics.get("queuedReportCount"))),
        ("diagnostics_dropped_pending", diagnostics.get("droppedPendingCount", 0) > 0, str(diagnostics.get("droppedPendingCount"))),
        ("diagnostics_last_report_applied", last_report.get("applied") is True, str(last_report)),
        ("diagnostics_last_execution_flushed", execution.get("flushed") is True, str(execution)),
        (
            "diagnostics_latest_resize_viewport",
            not resize_during_hold or viewport_columns == resize_wide_cols,
            f"resize_during_hold={resize_during_hold} viewport_columns={viewport_columns} expected={resize_wide_cols}",
        ),
        ("diagnostics_no_terminal_op_budget_overflow", execution.get("terminalOpBudgetExceeded") is False, str(execution)),
        ("alt_screen_balanced", alt_enters == alt_leaves, f"alt_enters={alt_enters} alt_leaves={alt_leaves}"),
        ("sync_update_used", sync_enters > 0, f"sync_enters={sync_enters}"),
        ("sync_update_balanced", sync_enters == sync_leaves, f"sync_enters={sync_enters} sync_leaves={sync_leaves}"),
        ("bracketed_paste_balanced", bracketed_enters == bracketed_leaves, f"bracketed={bracketed_enters}/{bracketed_leaves}"),
        ("no_repeated_fullscreen_clear", full_clears == 0, f"full_clears={full_clears}"),
        ("output_size_bounded", len(output) < 3_500_000, f"bytes={len(output)}"),
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
            "actions": str(actions_path),
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
            "content_completed": snapshot["content_completed"],
            "completed": snapshot["completed"],
        },
        "actions": actions,
        "tail_hold_growth": tail_hold_growth,
        "narrow_resize_growth": narrow_resize_growth,
        "wide_resize_growth": wide_resize_growth,
        "restore_growth": restore_growth,
        "post_content_growth": post_content_growth,
        "diagnostics": {
            "path": str(diagnostics_path),
            "hasPendingDraw": diagnostics.get("hasPendingDraw"),
            "manualScrollActive": diagnostics.get("manualScrollActive"),
            "reportCount": diagnostics.get("reportCount"),
            "queuedReportCount": diagnostics.get("queuedReportCount"),
            "manualScrollPreservedReportCount": manual_hold_count,
            "manualScrollTeardownReleaseCount": teardown_release_count,
            "droppedPendingCount": diagnostics.get("droppedPendingCount"),
            "lastReportApplied": last_report.get("applied"),
            "lastExecutionFlushed": execution.get("flushed"),
            "lastExecutionViewportColumns": viewport_columns,
            "lastExecutionViewportWidthProfile": execution.get("viewportWidthProfile"),
        },
        "head_marker": HEAD_MARKER in text,
        "tail_marker": TAIL_MARKER in text,
        "sync_enters": sync_enters,
        "sync_leaves": sync_leaves,
        "bracketed_enters": bracketed_enters,
        "bracketed_leaves": bracketed_leaves,
        "full_clears": full_clears,
        "output_bytes": len(output),
        "artifacts": {
            "pty_output": str(text_path),
            "mock_requests": str(mock_path),
            "actions": str(actions_path),
            "diagnostics": str(diagnostics_path),
            "assertions": str(ctx.artifacts_dir / "assertions.json"),
        },
    }


def main() -> int:
    result = run_manual_scroll_tail_hold_smoke()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
