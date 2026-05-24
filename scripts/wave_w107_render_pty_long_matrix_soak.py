#!/usr/bin/env python3
from __future__ import annotations

import json
import math
import os
import pty
import re
import signal
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log
from wave_w106_render_pty_mouse_scroll_soak import (
    DEBUG_MOSSEN,
    RUN_MOSSEN,
    MockState,
    decode_output,
    free_port,
    read_pty,
    send_mouse_wheel,
    send_scrollbar_click,
    set_pty_size,
)

HEAD_MARKER = "PTY_LONG_MATRIX_HEAD_W107"
TAIL_MARKER = "PTY_LONG_MATRIX_TAIL_W107"
ROW_RE = re.compile(r"matrix-row-(\d{4})")


def make_handler(state: MockState, *, chunks: int, delay_ms: int):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            state.record(self.path, dict(self.headers), b"")
            payload = json.dumps(
                {
                    "object": "list",
                    "data": [{"id": "pty-long-matrix-model", "object": "model"}],
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
                f"matrix-row-{idx:04}: long external PTY soak keeps streaming, resize, and scroll stable.\n"
                for idx in range(chunks)
            )
            pieces.append(f"{TAIL_MARKER}\n")

            try:
                for piece in pieces:
                    payload = {
                        "id": "pty-long-matrix",
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
                    "id": "pty-long-matrix",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 16, "completion_tokens": chunks + 2},
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


def start_mock_server(chunks: int, delay_ms: int) -> tuple[HTTPServer, MockState, threading.Thread]:
    state = MockState()
    server = HTTPServer(("127.0.0.1", free_port()), make_handler(state, chunks=chunks, delay_ms=delay_ms))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, state, thread


def resize_child(master_fd: int, proc: subprocess.Popen[bytes], *, rows: int, cols: int) -> None:
    set_pty_size(master_fd, rows=rows, cols=cols)
    try:
        os.kill(proc.pid, signal.SIGWINCH)
    except ProcessLookupError:
        pass


def rows_in_segment(segment: str) -> list[int]:
    return sorted(set(int(match.group(1)) for match in ROW_RE.finditer(segment)))


def compact_snapshot(snapshot: dict[str, Any]) -> dict[str, Any]:
    return {
        "request_count": len(snapshot["requests"]),
        "paths": [req.get("path", "") for req in snapshot["requests"]],
        "chunks_sent": snapshot["chunks_sent"],
        "completed": snapshot["completed"],
    }


def command_for_run() -> list[str]:
    force_build = os.environ.get("MOSSEN_PTY_LONG_MATRIX_FORCE_BUILD") == "1"
    if DEBUG_MOSSEN.exists() and os.access(DEBUG_MOSSEN, os.X_OK) and not force_build:
        return [str(DEBUG_MOSSEN), "--bare"]
    return [str(RUN_MOSSEN), "--bare"]


def run_long_matrix_soak() -> dict[str, Any]:
    if not RUN_MOSSEN.exists():
        raise RuntimeError(f"missing launcher: {RUN_MOSSEN}")

    ctx = make_fixture("W107_render_pty_long_matrix_soak")
    project = ctx.root_dir / "project"
    project.mkdir(parents=True, exist_ok=True)

    min_stream_secs = float(os.environ.get("MOSSEN_PTY_LONG_MATRIX_MIN_STREAM_SECS", "10"))
    delay_ms = int(os.environ.get("MOSSEN_PTY_LONG_MATRIX_DELAY_MS", "20"))
    min_chunks = max(120, math.ceil(min_stream_secs * 1000 / max(delay_ms, 1)))
    chunks = int(os.environ.get("MOSSEN_PTY_LONG_MATRIX_CHUNKS", str(min_chunks)))
    server, mock_state, thread = start_mock_server(chunks, delay_ms)
    port = server.server_address[1]
    base_url = f"http://127.0.0.1:{port}/v1"

    env = ctx.env.copy()
    env.update(
        {
            "MOSSEN_CONFIG_DIR": str(ctx.mossen_config_home),
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_BASE_URL": base_url,
            "MOSSEN_CODE_CUSTOM_MODEL": "pty-long-matrix-model",
            "MOSSEN_CODE_CUSTOM_NAME": "PTY Long Matrix Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-pty-long-matrix-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_DISABLE_THINKING": "1",
            "MOSSEN_CODE_DISABLE_ADAPTIVE_THINKING": "1",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "45",
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

    matrix_sizes = [(30, 118), (14, 58), (34, 132), (18, 72), (24, 96)]
    current_rows, current_cols = matrix_sizes[0]
    master_fd, slave_fd = pty.openpty()
    set_pty_size(slave_fd, rows=current_rows, cols=current_cols)

    output = bytearray()
    actions: list[dict[str, Any]] = []
    proc: subprocess.Popen[bytes] | None = None
    sent_prompt = False
    sent_manual_scroll = False
    sent_restore = False
    sent_scrollbar_top = False
    sent_final_restore = False
    sent_quit = False
    manual_offset: int | None = None
    restore_offset: int | None = None
    scrollbar_offset: int | None = None
    final_restore_offset: int | None = None
    resize_thresholds = [
        (int(chunks * 0.18), 1, "resize_narrow"),
        (int(chunks * 0.36), 2, "resize_wide_manual"),
        (int(chunks * 0.56), 3, "resize_compact_manual"),
        (int(chunks * 0.74), 4, "resize_medium_manual"),
    ]
    sent_resizes: set[str] = set()
    started = time.time()
    stream_started_at: float | None = None
    stream_completed_at: float | None = None
    timeout = float(os.environ.get("MOSSEN_PTY_LONG_MATRIX_TIMEOUT_SECS", "90"))
    command = command_for_run()

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
        prompt = "Run a longer PTY streaming matrix with resize and scroll interactions."

        while time.time() - started < timeout:
            read_pty(master_fd, output, timeout=0.04)
            text = decode_output(output)
            snapshot = mock_state.snapshot()
            chunks_sent = snapshot["chunks_sent"]
            if chunks_sent > 0 and stream_started_at is None:
                stream_started_at = time.time()
            if snapshot["completed"] and stream_completed_at is None:
                stream_completed_at = time.time()

            if not sent_prompt and ("\x1b[?1049h" in text or "send" in text or "Mossen" in text):
                os.write(master_fd, (prompt + "\r").encode("utf-8"))
                sent_prompt = True
                actions.append({"name": "prompt", "offset": len(output), "ts": time.time()})

            for threshold, size_index, name in resize_thresholds:
                if sent_prompt and name not in sent_resizes and chunks_sent >= threshold and proc.poll() is None:
                    current_rows, current_cols = matrix_sizes[size_index]
                    resize_child(master_fd, proc, rows=current_rows, cols=current_cols)
                    sent_resizes.add(name)
                    actions.append(
                        {
                            "name": name,
                            "rows": current_rows,
                            "cols": current_cols,
                            "chunks_sent": chunks_sent,
                            "offset": len(output),
                            "ts": time.time(),
                        }
                    )

            if sent_prompt and not sent_manual_scroll and chunks_sent >= int(chunks * 0.26):
                manual_offset = len(output)
                send_mouse_wheel(master_fd, down=False, col0=current_cols - 2, row0=current_rows // 2, repeat=30)
                sent_manual_scroll = True
                actions.append({"name": "mouse_wheel_up_manual", "offset": manual_offset, "ts": time.time()})

            if sent_manual_scroll and not sent_restore and snapshot["completed"]:
                for _ in range(15):
                    read_pty(master_fd, output, timeout=0.05)
                restore_offset = len(output)
                send_mouse_wheel(master_fd, down=True, col0=current_cols - 2, row0=current_rows // 2, repeat=280)
                sent_restore = True
                actions.append({"name": "mouse_wheel_down_restore", "offset": restore_offset, "ts": time.time()})

            if sent_restore and not sent_scrollbar_top:
                restore_segment = decode_output(output[restore_offset or 0 :])
                if TAIL_MARKER in restore_segment:
                    scrollbar_offset = len(output)
                    for row0 in range(1, max(3, min(current_rows - 4, 7))):
                        send_scrollbar_click(master_fd, col0=current_cols - 1, row0=row0)
                    sent_scrollbar_top = True
                    actions.append({"name": "scrollbar_top_click", "offset": scrollbar_offset, "ts": time.time()})

            if sent_scrollbar_top and not sent_final_restore:
                for _ in range(10):
                    read_pty(master_fd, output, timeout=0.05)
                final_restore_offset = len(output)
                send_mouse_wheel(master_fd, down=True, col0=current_cols - 2, row0=current_rows // 2, repeat=320)
                sent_final_restore = True
                actions.append({"name": "final_mouse_wheel_down", "offset": final_restore_offset, "ts": time.time()})

            if sent_final_restore and not sent_quit:
                final_segment = decode_output(output[final_restore_offset or 0 :])
                if TAIL_MARKER in final_segment:
                    os.write(master_fd, b"/quit\r")
                    sent_quit = True
                    actions.append({"name": "quit", "offset": len(output), "ts": time.time()})

            if sent_quit and proc.poll() is not None:
                break
            if proc.poll() is not None and sent_prompt:
                break

        if proc.poll() is None:
            if not sent_quit:
                try:
                    os.write(master_fd, b"/quit\r")
                    sent_quit = True
                    actions.append({"name": "quit_fallback", "offset": len(output), "ts": time.time()})
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

    text = decode_output(output)
    raw_path = ctx.artifacts_dir / "pty_raw_output.bin"
    text_path = ctx.artifacts_dir / "pty_output.txt"
    mock_path = ctx.artifacts_dir / "mock_requests.json"
    actions_path = ctx.artifacts_dir / "actions.json"
    raw_path.write_bytes(bytes(output))
    text_path.write_text(text, encoding="utf-8", errors="replace")
    mock_path.write_text(json.dumps(mock_state.snapshot(), indent=2, ensure_ascii=False))
    actions_path.write_text(json.dumps(actions, indent=2, ensure_ascii=False))

    exit_code = proc.returncode if proc is not None and proc.returncode is not None else -1
    write_command_log(ctx, command, text, "", exit_code)

    snapshot = mock_state.snapshot()
    chat_hit = any(req.get("path", "").endswith("/chat/completions") for req in snapshot["requests"])
    manual_segment = decode_output(output[manual_offset or 0 : restore_offset or len(output)])
    restore_segment = decode_output(output[restore_offset or 0 : scrollbar_offset or len(output)])
    scrollbar_segment = decode_output(output[scrollbar_offset or 0 : final_restore_offset or len(output)])
    final_segment = decode_output(output[final_restore_offset or 0 :])
    manual_rows = rows_in_segment(manual_segment)
    scrollbar_rows = rows_in_segment(scrollbar_segment)
    manual_min = manual_rows[0] if manual_rows else None
    manual_max = manual_rows[-1] if manual_rows else None
    scrollbar_min = scrollbar_rows[0] if scrollbar_rows else None
    scrollbar_max = scrollbar_rows[-1] if scrollbar_rows else None
    manual_history_visible = HEAD_MARKER in manual_segment or (
        manual_min is not None and manual_min <= max(20, chunks // 2)
    )
    scrollbar_history_visible = HEAD_MARKER in scrollbar_segment or (
        scrollbar_min is not None and scrollbar_min <= max(20, chunks // 2)
    )
    stream_elapsed = (
        stream_completed_at - stream_started_at
        if stream_started_at is not None and stream_completed_at is not None
        else 0.0
    )
    mouse_enable_count = sum(
        text.count(seq)
        for seq in ["\x1b[?1000h", "\x1b[?1002h", "\x1b[?1003h", "\x1b[?1006h"]
    )
    mouse_disable_count = sum(
        text.count(seq)
        for seq in ["\x1b[?1000l", "\x1b[?1002l", "\x1b[?1003l", "\x1b[?1006l"]
    )
    alt_enters = text.count("\x1b[?1049h")
    alt_leaves = text.count("\x1b[?1049l")
    full_clears = text.count("\x1b[2J") + text.count("\x1b[3J")
    output_bytes = len(output)

    assertions = [
        ("process_exited_zero", exit_code == 0, f"exit={exit_code}"),
        ("prompt_sent", sent_prompt, f"sent_prompt={sent_prompt}"),
        ("mock_chat_completion_hit", chat_hit, f"requests={len(snapshot['requests'])}"),
        ("mock_streamed_matrix_chunks", snapshot["chunks_sent"] >= chunks, f"chunks={snapshot['chunks_sent']}"),
        ("mock_completed_stream", snapshot["completed"], f"chunks={snapshot['chunks_sent']}"),
        ("stream_duration_floor", stream_elapsed >= min_stream_secs, f"stream_elapsed={stream_elapsed:.2f}s"),
        ("all_matrix_resizes_sent", len(sent_resizes) == len(resize_thresholds), f"resizes={sorted(sent_resizes)}"),
        ("manual_mouse_scroll_sent", sent_manual_scroll, "wheel-up while stream active"),
        ("manual_scroll_preserved_history", manual_history_visible, f"manual rows min={manual_min} max={manual_max}"),
        ("manual_scroll_hid_final_tail", TAIL_MARKER not in manual_segment, "tail absent before restore"),
        ("restore_rendered_tail", TAIL_MARKER in restore_segment, TAIL_MARKER),
        ("scrollbar_top_click_sent", sent_scrollbar_top, "scrollbar rail click after restore"),
        (
            "scrollbar_click_preserved_history",
            scrollbar_history_visible,
            f"scrollbar rows min={scrollbar_min} max={scrollbar_max}",
        ),
        ("scrollbar_click_hid_tail", TAIL_MARKER not in scrollbar_segment, "tail absent after scrollbar click"),
        ("final_restore_rendered_tail", TAIL_MARKER in final_segment, TAIL_MARKER),
        ("mouse_capture_enabled", mouse_enable_count > 0, f"mouse_enable_count={mouse_enable_count}"),
        ("mouse_capture_disabled", mouse_disable_count > 0, f"mouse_disable_count={mouse_disable_count}"),
        ("entered_alt_screen_once", alt_enters == 1, f"alt_enters={alt_enters}"),
        ("left_alt_screen_once", alt_leaves == 1, f"alt_leaves={alt_leaves}"),
        ("no_repeated_fullscreen_clear", full_clears <= 4, f"full_clears={full_clears}"),
        ("output_size_bounded", output_bytes < 8_000_000, f"bytes={output_bytes}"),
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
        },
    )

    return {
        "ok": all(passed for _, passed, _ in assertions),
        "fixture_root": str(ctx.root_dir),
        "exit_code": exit_code,
        "sent_prompt": sent_prompt,
        "sent_resizes": sorted(sent_resizes),
        "sent_manual_scroll": sent_manual_scroll,
        "sent_restore": sent_restore,
        "sent_scrollbar_top": sent_scrollbar_top,
        "sent_final_restore": sent_final_restore,
        "sent_quit": sent_quit,
        "mock": compact_snapshot(snapshot),
        "stream_elapsed": round(stream_elapsed, 3),
        "manual_rows": {
            "min": manual_min,
            "max": manual_max,
            "count": len(manual_rows),
        },
        "scrollbar_rows": {
            "min": scrollbar_min,
            "max": scrollbar_max,
            "count": len(scrollbar_rows),
        },
        "mouse_capture": {
            "enable_count": mouse_enable_count,
            "disable_count": mouse_disable_count,
        },
        "alt_enters": alt_enters,
        "alt_leaves": alt_leaves,
        "full_clears": full_clears,
        "output_bytes": output_bytes,
        "artifacts": {
            "pty_output": str(text_path),
            "mock_requests": str(mock_path),
            "actions": str(actions_path),
            "assertions": str(ctx.artifacts_dir / "assertions.json"),
        },
    }


def main() -> int:
    result = run_long_matrix_soak()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
