#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import pty
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
from wave_w293_terminal_manual_scroll_approval_bypass_pty_smoke import (
    DEBUG_MOSSEN,
    decode_output,
    free_port,
    read_pty,
    send_key,
    set_pty_size,
)
from wave_w106_render_pty_mouse_scroll_soak import send_mouse_wheel

HEAD_MARKER = "TERMINAL_APPROVAL_REJECT_HEAD_W294"
TAIL_MARKER = "TERMINAL_APPROVAL_REJECT_TAIL_W294"
FINAL_MARKER = "TERMINAL_APPROVAL_REJECT_FINAL_W294"
SENTINEL_PATH = Path("/tmp/mossen_terminal_approval_reject_w294")
APPROVAL_COMMAND = f"touch {SENTINEL_PATH}"


@dataclass
class MockState:
    requests: list[dict[str, Any]] = field(default_factory=list)
    chat_post_count: int = 0
    chunks_sent: int = 0
    content_completed: bool = False
    tool_call_sent: bool = False
    first_stream_completed: bool = False
    final_response_sent: bool = False
    final_stream_completed: bool = False
    lock: threading.Lock = field(default_factory=threading.Lock)

    def record_get(self, path: str, headers: dict[str, str]) -> None:
        self._record(path, headers, b"", None)

    def record_post(self, path: str, headers: dict[str, str], body: bytes) -> int:
        with self.lock:
            self.chat_post_count += 1
            chat_index = self.chat_post_count
        self._record(path, headers, body, chat_index)
        return chat_index

    def _record(
        self,
        path: str,
        headers: dict[str, str],
        body: bytes,
        chat_index: int | None,
    ) -> None:
        body_text = body.decode("utf-8", errors="replace")
        with self.lock:
            self.requests.append(
                {
                    "path": path,
                    "chat_index": chat_index,
                    "authorization": headers.get("Authorization", ""),
                    "x_api_key": headers.get("x-api-key", ""),
                    "body": body_text[:6000],
                    "body_tail": body_text[-12000:],
                    "body_len": len(body_text),
                    "ts": time.time(),
                }
            )

    def mark_chunk(self) -> None:
        with self.lock:
            self.chunks_sent += 1

    def mark_content_completed(self) -> None:
        with self.lock:
            self.content_completed = True

    def mark_tool_call_sent(self) -> None:
        with self.lock:
            self.tool_call_sent = True

    def mark_first_stream_completed(self) -> None:
        with self.lock:
            self.first_stream_completed = True

    def mark_final_response_sent(self) -> None:
        with self.lock:
            self.final_response_sent = True

    def mark_final_stream_completed(self) -> None:
        with self.lock:
            self.final_stream_completed = True

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "requests": list(self.requests),
                "chat_post_count": self.chat_post_count,
                "chunks_sent": self.chunks_sent,
                "content_completed": self.content_completed,
                "tool_call_sent": self.tool_call_sent,
                "first_stream_completed": self.first_stream_completed,
                "final_response_sent": self.final_response_sent,
                "final_stream_completed": self.final_stream_completed,
            }


def write_sse(wfile: Any, payload: dict[str, Any]) -> None:
    wfile.write(f"data: {json.dumps(payload)}\n\n".encode("utf-8"))
    wfile.flush()


def make_handler(state: MockState, *, chunks: int, delay_ms: int, approval_pause_ms: int):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            state.record_get(self.path, dict(self.headers))
            payload = json.dumps(
                {
                    "object": "list",
                    "data": [{"id": "terminal-approval-reject-model", "object": "model"}],
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
            chat_index = state.record_post(self.path, dict(self.headers), body)
            if not self.path.endswith("/chat/completions"):
                self.send_response(404)
                self.end_headers()
                return

            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()

            try:
                if chat_index == 1:
                    self._write_first_turn()
                else:
                    self._write_final_turn()
            except (BrokenPipeError, ConnectionResetError):
                return
            finally:
                self.close_connection = True

        def _write_first_turn(self) -> None:
            pieces = [f"{HEAD_MARKER}\n"]
            pieces.extend(
                f"approval-reject-row-{idx:03}: manual scroll must still allow reject submission.\n"
                for idx in range(chunks)
            )
            pieces.append(f"{TAIL_MARKER}\n")

            for piece in pieces:
                write_sse(
                    self.wfile,
                    {
                        "id": "terminal-approval-reject",
                        "object": "chat.completion.chunk",
                        "choices": [
                            {
                                "index": 0,
                                "delta": {"content": piece},
                                "finish_reason": None,
                            }
                        ],
                    },
                )
                state.mark_chunk()
                time.sleep(delay_ms / 1000.0)

            state.mark_content_completed()
            time.sleep(approval_pause_ms / 1000.0)
            arguments = json.dumps({"command": APPROVAL_COMMAND})
            write_sse(
                self.wfile,
                {
                    "id": "terminal-approval-reject",
                    "object": "chat.completion.chunk",
                    "choices": [
                        {
                            "index": 0,
                            "delta": {
                                "tool_calls": [
                                    {
                                        "index": 0,
                                        "id": "call_terminal_approval_reject_w294",
                                        "type": "function",
                                        "function": {
                                            "name": "Bash",
                                            "arguments": arguments,
                                        },
                                    }
                                ]
                            },
                            "finish_reason": None,
                        }
                    ],
                },
            )
            state.mark_tool_call_sent()

            write_sse(
                self.wfile,
                {
                    "id": "terminal-approval-reject",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}],
                    "usage": {"prompt_tokens": 12, "completion_tokens": chunks + 2},
                },
            )
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            state.mark_first_stream_completed()

        def _write_final_turn(self) -> None:
            body = f"{FINAL_MARKER}\npermission rejection was submitted without executing the command.\n"
            write_sse(
                self.wfile,
                {
                    "id": "terminal-approval-reject-final",
                    "object": "chat.completion.chunk",
                    "choices": [
                        {
                            "index": 0,
                            "delta": {"content": body},
                            "finish_reason": None,
                        }
                    ],
                },
            )
            state.mark_final_response_sent()
            write_sse(
                self.wfile,
                {
                    "id": "terminal-approval-reject-final",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 20, "completion_tokens": 8},
                },
            )
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            state.mark_final_stream_completed()

        def log_message(self, _format: str, *_args: Any) -> None:
            return

    return Handler


def start_mock_server(
    chunks: int, delay_ms: int, approval_pause_ms: int
) -> tuple[HTTPServer, MockState, threading.Thread]:
    state = MockState()
    server = HTTPServer(
        ("127.0.0.1", free_port()),
        make_handler(state, chunks=chunks, delay_ms=delay_ms, approval_pause_ms=approval_pause_ms),
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, state, thread


def run_manual_scroll_approval_reject_smoke() -> dict[str, Any]:
    fixture_name = os.environ.get(
        "MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_FIXTURE_NAME",
        "W294_terminal_manual_scroll_approval_reject_pty_smoke",
    )
    ctx = make_fixture(fixture_name)
    project = ctx.root_dir / "project"
    project.mkdir(parents=True, exist_ok=True)
    try:
        SENTINEL_PATH.unlink()
    except FileNotFoundError:
        pass

    chunks = int(os.environ.get("MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_CHUNKS", "92"))
    delay_ms = int(os.environ.get("MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_DELAY_MS", "5"))
    approval_pause_ms = int(os.environ.get("MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_APPROVAL_PAUSE_MS", "700"))
    scroll_after_approval = (
        os.environ.get("MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_SCROLL_AFTER_APPROVAL", "0")
        != "0"
    )
    mouse_scroll_after_approval = (
        os.environ.get(
            "MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_MOUSE_SCROLL_AFTER_APPROVAL", "0"
        )
        != "0"
    )
    active_scroll_after_approval = scroll_after_approval or mouse_scroll_after_approval
    active_scroll_input_delay_secs = float(
        os.environ.get("MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_ACTIVE_SCROLL_DELAY_SECS", "0.12")
    )
    server, mock_state, thread = start_mock_server(chunks, delay_ms, approval_pause_ms)
    port = server.server_address[1]
    base_url = f"http://127.0.0.1:{port}/v1"
    diagnostics_path = ctx.artifacts_dir / "terminal_render_diagnostics.json"

    env = ctx.env.copy()
    env.update(
        {
            "MOSSEN_CONFIG_DIR": str(ctx.mossen_config_home),
            "MOSSEN_PERMISSION_MODE": "default",
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_BASE_URL": base_url,
            "MOSSEN_CODE_CUSTOM_MODEL": "terminal-approval-reject-model",
            "MOSSEN_CODE_CUSTOM_NAME": "Terminal Approval Reject Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-terminal-approval-reject-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_DISABLE_THINKING": "1",
            "MOSSEN_CODE_DISABLE_ADAPTIVE_THINKING": "1",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "30",
            "MOSSEN_TERMINAL_RENDER_DIAGNOSTICS_PATH": str(diagnostics_path),
            "MOSSEN_TERMINAL_RENDER_CAPTURE_MOUSE": (
                "1" if mouse_scroll_after_approval else "0"
            ),
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

    skip_build = os.environ.get("MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_SKIP_BUILD") == "1"
    if not skip_build:
        build_timeout = float(os.environ.get("MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_BUILD_TIMEOUT_SECS", "300"))
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
                "failed to build mossen-cli for W294 terminal approval reject PTY smoke; "
                f"see {ctx.artifacts_dir / 'build_stderr.txt'}"
            )
    elif not (DEBUG_MOSSEN.exists() and os.access(DEBUG_MOSSEN, os.X_OK)):
        raise RuntimeError(f"missing debug binary with build skipped: {DEBUG_MOSSEN}")

    command = [
        str(DEBUG_MOSSEN),
        "--bare",
        "--oneshot",
        "Stream terminal manual-scroll approval reject diagnostics markers and request Bash approval.",
        "--emit",
        "terminal",
    ]

    master_fd, slave_fd = pty.openpty()
    set_pty_size(slave_fd, rows=24, cols=96)
    output = bytearray()
    actions: list[dict[str, Any]] = []
    proc: subprocess.Popen[bytes] | None = None
    started = time.time()
    timeout = float(os.environ.get("MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_TIMEOUT_SECS", "90"))
    sent_page_up = False
    sent_active_scroll_after_approval = False
    sent_focus_reject_after_scroll = False
    sent_reject = False
    sent_interrupt = False
    page_up_offset: int | None = None
    content_complete_offset: int | None = None
    approval_offset: int | None = None
    active_scroll_offset: int | None = None
    focus_reject_offset: int | None = None
    reject_focus_visible_offset: int | None = None
    reject_offset: int | None = None
    submitted_offset: int | None = None
    final_offset: int | None = None
    active_scroll_at: float | None = None

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
            text_so_far = decode_output(output)
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

            if sent_page_up and content_complete_offset is None and snapshot["content_completed"]:
                content_complete_offset = len(output)
                actions.append(
                    {
                        "name": "content_complete_while_manual_scroll",
                        "chunks_sent": chunks_sent,
                        "offset": content_complete_offset,
                    }
                )

            approval_visible = (
                "approval required" in text_so_far
                and "tool: Bash" in text_so_far
                and APPROVAL_COMMAND in text_so_far
            )
            if sent_page_up and approval_offset is None and approval_visible:
                approval_offset = len(output)
                actions.append(
                    {
                        "name": "approval_visible_without_scroll_restore",
                        "chunks_sent": chunks_sent,
                        "offset": approval_offset,
                    }
                )

            if (
                active_scroll_after_approval
                and approval_offset is not None
                and not sent_active_scroll_after_approval
                and proc.poll() is None
            ):
                if mouse_scroll_after_approval:
                    send_mouse_wheel(
                        master_fd,
                        down=False,
                        col0=94,
                        row0=12,
                        repeat=8,
                    )
                else:
                    send_key(master_fd, b"\x1b[5~", repeat=8)
                sent_active_scroll_after_approval = True
                active_scroll_offset = len(output)
                active_scroll_at = time.time()
                actions.append(
                    {
                        "name": (
                            "mouse_wheel_up_while_approval_active"
                            if mouse_scroll_after_approval
                            else "page_up_while_approval_active"
                        ),
                        "chunks_sent": chunks_sent,
                        "offset": active_scroll_offset,
                    }
                )

            if (
                active_scroll_after_approval
                and sent_active_scroll_after_approval
                and not sent_focus_reject_after_scroll
                and active_scroll_at is not None
                and (time.time() - active_scroll_at) >= active_scroll_input_delay_secs
                and proc.poll() is None
            ):
                send_key(master_fd, b"\x1b[C", repeat=1)
                sent_focus_reject_after_scroll = True
                focus_reject_offset = len(output)
                actions.append(
                    {
                        "name": "focus_reject_after_active_scroll",
                        "chunks_sent": chunks_sent,
                        "offset": focus_reject_offset,
                    }
                )

            text_after_focus = (
                decode_output(output[focus_reject_offset:]) if focus_reject_offset is not None else ""
            )
            reject_focus_visible = "[>Reject<]" in text_after_focus
            if (
                active_scroll_after_approval
                and sent_focus_reject_after_scroll
                and reject_focus_visible_offset is None
                and reject_focus_visible
            ):
                reject_focus_visible_offset = len(output)
                actions.append(
                    {
                        "name": "reject_focus_visible_after_active_scroll",
                        "chunks_sent": chunks_sent,
                        "offset": reject_focus_visible_offset,
                    }
                )

            reject_ready = approval_offset is not None and (
                not active_scroll_after_approval or reject_focus_visible_offset is not None
            )
            if reject_ready and not sent_reject and proc.poll() is None:
                send_key(master_fd, b"\r" if active_scroll_after_approval else b"n", repeat=1)
                sent_reject = True
                reject_offset = len(output)
                actions.append(
                    {
                        "name": (
                            "reject_enter_after_active_scroll"
                            if active_scroll_after_approval
                            else "reject_key_after_approval"
                        ),
                        "chunks_sent": chunks_sent,
                        "offset": reject_offset,
                    }
                )

            submitted_visible = (
                "approval submitted: Reject" in text_so_far
                or "selected: Reject (submitted)" in text_so_far
            )
            if submitted_visible and submitted_offset is None:
                submitted_offset = len(output)
                actions.append(
                    {
                        "name": "reject_submission_rendered",
                        "chunks_sent": chunks_sent,
                        "offset": submitted_offset,
                    }
                )

            if FINAL_MARKER in text_so_far and final_offset is None:
                final_offset = len(output)
                actions.append(
                    {
                        "name": "final_response_rendered_after_reject",
                        "chunks_sent": chunks_sent,
                        "offset": final_offset,
                    }
                )

            if proc.poll() is not None:
                break

        if proc.poll() is None:
            try:
                send_key(master_fd, b"\x03", repeat=1)
                sent_interrupt = True
                actions.append(
                    {
                        "name": "ctrl_c_timeout_fallback",
                        "chunks_sent": mock_state.snapshot()["chunks_sent"],
                        "offset": len(output),
                    }
                )
            except OSError:
                pass
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
    chat_posts = [
        req for req in snapshot["requests"] if req.get("path", "").endswith("/chat/completions")
    ]
    alt_enters = text.count("\x1b[?1049h")
    alt_leaves = text.count("\x1b[?1049l")
    sync_enters = text.count("\x1b[?2026h")
    sync_leaves = text.count("\x1b[?2026l")
    bracketed_enters = text.count("\x1b[?2004h")
    bracketed_leaves = text.count("\x1b[?2004l")
    mouse_enable_count = sum(
        text.count(seq)
        for seq in ["\x1b[?1000h", "\x1b[?1002h", "\x1b[?1003h", "\x1b[?1006h"]
    )
    mouse_disable_count = sum(
        text.count(seq)
        for seq in ["\x1b[?1000l", "\x1b[?1002l", "\x1b[?1003l", "\x1b[?1006l"]
    )
    full_clears = text.count("\x1b[2J") + text.count("\x1b[3J")
    last_report = diagnostics.get("lastReport") or {}
    execution = last_report.get("execution") or {}
    manual_hold_count = diagnostics.get("manualScrollPreservedReportCount", 0)
    tail_hold_growth = None
    if page_up_offset is not None and content_complete_offset is not None:
        tail_hold_growth = content_complete_offset - page_up_offset
    approval_bypass_growth = None
    if content_complete_offset is not None and approval_offset is not None:
        approval_bypass_growth = approval_offset - content_complete_offset
    text_after_final = text[final_offset:] if final_offset is not None else ""
    active_scroll_growth = None
    if approval_offset is not None and active_scroll_offset is not None:
        active_scroll_growth = active_scroll_offset - approval_offset

    assertions = [
        ("process_exited_zero", exit_code == 0, f"exit={exit_code} sent_interrupt={sent_interrupt}"),
        ("mock_two_chat_turns", len(chat_posts) >= 2, f"chat_posts={len(chat_posts)}"),
        ("mock_streamed_chunks", snapshot["chunks_sent"] >= chunks + 2, f"chunks={snapshot['chunks_sent']}"),
        ("mock_content_completed", snapshot["content_completed"], str(snapshot)),
        ("mock_tool_call_sent", snapshot["tool_call_sent"], str(snapshot)),
        ("mock_first_stream_completed", snapshot["first_stream_completed"], str(snapshot)),
        ("mock_final_response_sent", snapshot["final_response_sent"], str(snapshot)),
        ("mock_final_stream_completed", snapshot["final_stream_completed"], str(snapshot)),
        ("page_up_sent", sent_page_up, str(actions)),
        ("content_complete_seen_while_manual_scroll", content_complete_offset is not None, str(actions)),
        (
            "tail_output_held_before_approval",
            tail_hold_growth is not None and tail_hold_growth <= 1024,
            f"tail_hold_growth={tail_hold_growth} page_up_offset={page_up_offset} content_complete_offset={content_complete_offset}",
        ),
        ("approval_visible_without_scroll_restore", approval_offset is not None, str(actions)),
        (
            "approval_bypassed_manual_scroll_hold",
            approval_bypass_growth is not None and approval_bypass_growth > 256,
            f"approval_bypass_growth={approval_bypass_growth} content_complete_offset={content_complete_offset} approval_offset={approval_offset}",
        ),
        ("approval_tool_rendered", "tool: Bash" in text, "tool: Bash"),
        ("approval_command_preview_rendered", APPROVAL_COMMAND in text, APPROVAL_COMMAND),
        (
            "approval_active_scroll_policy_observed",
            (
                sent_active_scroll_after_approval
                and sent_focus_reject_after_scroll
                and reject_focus_visible_offset is not None
                if active_scroll_after_approval
                else not sent_active_scroll_after_approval
                and not sent_focus_reject_after_scroll
            ),
            f"active_scroll_after_approval={active_scroll_after_approval} mouse_scroll_after_approval={mouse_scroll_after_approval} actions={actions}",
        ),
        (
            "approval_focus_survived_active_scroll",
            not active_scroll_after_approval or reject_focus_visible_offset is not None,
            f"active_scroll_growth={active_scroll_growth} actions={actions}",
        ),
        (
            "mouse_capture_enabled_for_mouse_scroll",
            not mouse_scroll_after_approval or mouse_enable_count > 0,
            f"mouse_enable_count={mouse_enable_count}",
        ),
        (
            "mouse_capture_disabled_for_mouse_scroll",
            not mouse_scroll_after_approval or mouse_disable_count > 0,
            f"mouse_disable_count={mouse_disable_count}",
        ),
        (
            "reject_sent_after_approval",
            sent_reject and approval_offset is not None,
            str(actions),
        ),
        (
            "approval_region_not_stuck_after_reject",
            final_offset is not None and "approval required" not in text_after_final,
            f"final_offset={final_offset} text_after_final={text_after_final[:1000]}",
        ),
        ("final_response_rendered_after_reject", final_offset is not None, str(actions)),
        ("command_not_executed", not SENTINEL_PATH.exists(), str(SENTINEL_PATH)),
        ("diagnostics_file_written", diagnostics_path.exists(), str(diagnostics_path)),
        ("diagnostics_json_parseable", bool(diagnostics) and not diagnostics_parse_error, diagnostics_parse_error),
        ("diagnostics_no_pending_draw", diagnostics.get("hasPendingDraw") is False, str(diagnostics.get("hasPendingDraw"))),
        ("diagnostics_manual_scroll_released", diagnostics.get("manualScrollActive") is False, str(diagnostics.get("manualScrollActive"))),
        ("diagnostics_manual_scroll_preserved_reports", manual_hold_count > 0, str(manual_hold_count)),
        ("diagnostics_last_report_applied", last_report.get("applied") is True, str(last_report)),
        ("diagnostics_last_execution_flushed", execution.get("flushed") is True, str(execution)),
        ("diagnostics_no_terminal_op_budget_overflow", execution.get("terminalOpBudgetExceeded") is False, str(execution)),
        ("alt_screen_balanced", alt_enters == alt_leaves, f"alt_enters={alt_enters} alt_leaves={alt_leaves}"),
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
            "chat_post_count": len(chat_posts),
            "chunks_sent": snapshot["chunks_sent"],
            "content_completed": snapshot["content_completed"],
            "tool_call_sent": snapshot["tool_call_sent"],
            "first_stream_completed": snapshot["first_stream_completed"],
            "final_response_sent": snapshot["final_response_sent"],
            "final_stream_completed": snapshot["final_stream_completed"],
        },
        "actions": actions,
        "tail_hold_growth": tail_hold_growth,
        "approval_bypass_growth": approval_bypass_growth,
        "scroll_after_approval": scroll_after_approval,
        "mouse_scroll_after_approval": mouse_scroll_after_approval,
        "active_scroll_growth": active_scroll_growth,
        "reject_focus_visible_after_active_scroll": reject_focus_visible_offset is not None,
        "command_executed": SENTINEL_PATH.exists(),
        "diagnostics": {
            "path": str(diagnostics_path),
            "hasPendingDraw": diagnostics.get("hasPendingDraw"),
            "manualScrollActive": diagnostics.get("manualScrollActive"),
            "reportCount": diagnostics.get("reportCount"),
            "queuedReportCount": diagnostics.get("queuedReportCount"),
            "manualScrollPreservedReportCount": manual_hold_count,
            "droppedPendingCount": diagnostics.get("droppedPendingCount"),
            "lastReportApplied": last_report.get("applied"),
            "lastExecutionFlushed": execution.get("flushed"),
            "lastExecutionViewportColumns": execution.get("viewportColumns"),
            "lastExecutionViewportWidthProfile": execution.get("viewportWidthProfile"),
        },
        "approval_rendered": "approval required" in text and "tool: Bash" in text,
        "final_rendered": final_offset is not None,
        "sync_enters": sync_enters,
        "sync_leaves": sync_leaves,
        "bracketed_enters": bracketed_enters,
        "bracketed_leaves": bracketed_leaves,
        "mouse_enable_count": mouse_enable_count,
        "mouse_disable_count": mouse_disable_count,
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
    result = run_manual_scroll_approval_reject_smoke()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
