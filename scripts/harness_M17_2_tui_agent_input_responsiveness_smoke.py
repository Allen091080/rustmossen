#!/usr/bin/env python3
"""M17.2 - TUI input responsiveness while a background Agent is running.

This reproduces the interactive failure class where an Agent is launched in the
background, TaskOutput waits for it, and the user types while the child is still
working. The pass condition is not just model success: the TUI event loop must
process key events during the background-agent window and the final child result
must be surfaced through TaskOutput.
"""

from __future__ import annotations

import json
import os
import pty
import re
import signal
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log
from harness_M10_4_async_agent_taskoutput_smoke import (
    AGENT_TOOL_NAME,
    CHILD_MARKER,
    PARENT_MARKER,
    TASK_OUTPUT_TOOL_NAME,
    iter_strings,
    parse_json_string,
    write_text_final,
    write_tool_call,
)
from wave_w106_render_pty_mouse_scroll_soak import (
    DEBUG_MOSSEN,
    RUN_MOSSEN,
    decode_output,
    free_port,
    read_pty,
    set_pty_size,
)

INPUT_PROBE_TEXT = "agent-input-probe-m17"
EVENT_KEYS_RE = re.compile(r"\bkeys=(\d+)\b")


class PtyAgentState:
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
            self.requests.append({"path": path, "body": parsed, "ts": time.time()})
            for text in iter_strings(parsed):
                if "Mossen sub-agent launched by a parent session" in text:
                    self.child_request_seen = True
                if "async_launched" in text:
                    self.async_agent_result_seen = True
                    maybe_json = parse_json_string(text)
                    if isinstance(maybe_json, dict) and isinstance(maybe_json.get("task_id"), str):
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


def make_handler(state: PtyAgentState, *, child_delay_secs: float):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            payload = json.dumps(
                {"object": "list", "data": [{"id": "m17-agent-input-model", "object": "model"}]}
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
                    time.sleep(child_delay_secs)
                    write_text_final(self.wfile, f"{CHILD_MARKER}: delayed child completed.")
                elif request_index == 1:
                    write_tool_call(
                        self.wfile,
                        "call_m17_agent",
                        AGENT_TOOL_NAME,
                        {
                            "description": "delayed child",
                            "prompt": f"Wait briefly, then return {CHILD_MARKER}.",
                            "subagent_type": "general-purpose",
                            "run_in_background": True,
                        },
                    )
                elif snapshot["captured_task_id"] and not snapshot["task_output_result_seen"]:
                    write_tool_call(
                        self.wfile,
                        "call_m17_task_output",
                        TASK_OUTPUT_TOOL_NAME,
                        {"task_id": snapshot["captured_task_id"], "block": True},
                    )
                else:
                    write_text_final(self.wfile, f"{PARENT_MARKER}: TaskOutput surfaced child result.")
            except (BrokenPipeError, ConnectionResetError):
                return

        def log_message(self, _format: str, *_args: Any) -> None:
            return

    return Handler


def start_mock_server(child_delay_secs: float) -> tuple[ThreadingHTTPServer, PtyAgentState, threading.Thread]:
    state = PtyAgentState()
    server = ThreadingHTTPServer(
        ("127.0.0.1", free_port()),
        make_handler(state, child_delay_secs=child_delay_secs),
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, state, thread


def command_for_run() -> list[str]:
    force_build = os.environ.get("MOSSEN_AGENT_INPUT_PROBE_FORCE_BUILD") == "1"
    if DEBUG_MOSSEN.exists() and os.access(DEBUG_MOSSEN, os.X_OK) and not force_build:
        base = [str(DEBUG_MOSSEN), "--bare"]
    else:
        base = [str(RUN_MOSSEN), "--bare"]
    return [
        *base,
        "--access-policy",
        "unrestricted",
        "--instruments",
        "Agent,TaskOutput",
    ]


def tui_event_key_count(path: Path) -> int:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return 0
    return sum(int(match.group(1)) for match in EVENT_KEYS_RE.finditer(text))


def wait_for_tui_key_count(
    path: Path,
    *,
    minimum: int,
    deadline_secs: float,
    master_fd: int,
    output: bytearray,
) -> int:
    deadline = time.time() + max(0.0, deadline_secs)
    observed = tui_event_key_count(path)
    while observed < minimum and time.time() < deadline:
        read_pty(master_fd, output, timeout=0.04)
        observed = tui_event_key_count(path)
    return observed


def run_probe() -> dict[str, Any]:
    ctx = make_fixture("M17.2_tui_agent_input_responsiveness")
    project = ctx.root_dir / "project"
    project.mkdir(parents=True, exist_ok=True)
    tui_event_log_path = ctx.artifacts_dir / "tui_events.log"
    child_delay_secs = float(os.environ.get("MOSSEN_AGENT_INPUT_PROBE_CHILD_DELAY_SECS", "6"))
    probe_event_wait_secs = float(os.environ.get("MOSSEN_AGENT_INPUT_PROBE_EVENT_WAIT_SECS", "3"))
    timeout = float(os.environ.get("MOSSEN_AGENT_INPUT_PROBE_TIMEOUT_SECS", "60"))
    server, state, thread = start_mock_server(child_delay_secs)
    port = server.server_address[1]
    base_url = f"http://127.0.0.1:{port}/v1"

    env = ctx.env.copy()
    env.update(
        {
            "MOSSEN_CONFIG_DIR": str(ctx.mossen_config_home),
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_BASE_URL": base_url,
            "MOSSEN_CODE_CUSTOM_MODEL": "m17-agent-input-model",
            "MOSSEN_CODE_CUSTOM_NAME": "M17 Agent Input Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-m17-agent-input-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_DISABLE_THINKING": "1",
            "MOSSEN_CODE_DISABLE_ADAPTIVE_THINKING": "1",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "45",
            "MOSSEN_TUI_EVENT_LOG_PATH": str(tui_event_log_path),
            "TERM": "xterm-256color",
            "TERM_PROGRAM": "WezTerm",
        }
    )

    real_home = Path(os.environ.get("HOME", str(Path.home())))
    cargo_home = Path(os.environ.get("CARGO_HOME", str(real_home / ".cargo")))
    rustup_home = Path(os.environ.get("RUSTUP_HOME", str(real_home / ".rustup")))
    if cargo_home.exists():
        env["CARGO_HOME"] = str(cargo_home)
    if rustup_home.exists():
        env["RUSTUP_HOME"] = str(rustup_home)

    rows, cols = 28, 112
    master_fd, slave_fd = pty.openpty()
    set_pty_size(slave_fd, rows=rows, cols=cols)
    output = bytearray()
    actions: list[dict[str, Any]] = []
    prompt = "Launch a background Agent, retrieve it with TaskOutput, then answer with the final marker."
    command = command_for_run()
    sent_prompt = False
    sent_input_probe = False
    sent_quit = False
    probe_key_events_before: int | None = None
    probe_key_events_after: int | None = None
    started = time.time()
    proc: subprocess.Popen[bytes] | None = None

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
            text = decode_output(output)
            snapshot = state.snapshot()

            if not sent_prompt and ("\x1b[?1049h" in text or "send" in text or "Mossen" in text):
                os.write(master_fd, (prompt + "\r").encode("utf-8"))
                sent_prompt = True
                actions.append({"name": "prompt", "offset": len(output), "ts": time.time()})

            if sent_prompt and not sent_input_probe and snapshot["child_request_seen"]:
                probe_key_events_before = tui_event_key_count(tui_event_log_path)
                typed_bytes = os.write(master_fd, INPUT_PROBE_TEXT.encode("utf-8"))
                probe_key_events_after = wait_for_tui_key_count(
                    tui_event_log_path,
                    minimum=probe_key_events_before + len(INPUT_PROBE_TEXT),
                    deadline_secs=probe_event_wait_secs,
                    master_fd=master_fd,
                    output=output,
                )
                erased_bytes = os.write(master_fd, b"\x7f" * len(INPUT_PROBE_TEXT))
                probe_key_events_after = wait_for_tui_key_count(
                    tui_event_log_path,
                    minimum=probe_key_events_before + len(INPUT_PROBE_TEXT),
                    deadline_secs=0.5,
                    master_fd=master_fd,
                    output=output,
                )
                sent_input_probe = True
                actions.append(
                    {
                        "name": "input_probe_while_child_running",
                        "text": INPUT_PROBE_TEXT,
                        "typed_bytes": typed_bytes,
                        "erased_bytes": erased_bytes,
                        "key_events_before": probe_key_events_before,
                        "key_events_after": probe_key_events_after,
                        "offset": len(output),
                        "request_count": snapshot["request_count"],
                        "ts": time.time(),
                    }
                )

            if PARENT_MARKER in text and not sent_quit:
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
    snapshot = state.snapshot()
    key_events_seen = tui_event_key_count(tui_event_log_path)
    probe_key_events_delta = (
        max(0, (probe_key_events_after or 0) - probe_key_events_before)
        if probe_key_events_before is not None
        else 0
    )
    minimum_expected_key_events = len(INPUT_PROBE_TEXT)
    exit_code = proc.returncode if proc is not None and proc.returncode is not None else -1

    raw_path = ctx.artifacts_dir / "pty_raw_output.bin"
    text_path = ctx.artifacts_dir / "pty_output.txt"
    mock_path = ctx.artifacts_dir / "mock_requests.json"
    actions_path = ctx.artifacts_dir / "actions.json"
    evidence_path = ctx.artifacts_dir / "input-responsive-evidence.json"
    raw_path.write_bytes(bytes(output))
    text_path.write_text(text, encoding="utf-8", errors="replace")
    mock_path.write_text(json.dumps(snapshot, indent=2, ensure_ascii=False), encoding="utf-8")
    actions_path.write_text(json.dumps(actions, indent=2, ensure_ascii=False), encoding="utf-8")
    evidence_path.write_text(
        json.dumps(
            {
                "ok": sent_input_probe and probe_key_events_delta >= minimum_expected_key_events,
                "method": "pty_background_agent_input_probe",
                "observed_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "during_background_agent_work": True,
                "observations": [
                    {
                        "action": "typed and erased probe text while child Agent request was in flight",
                        "result": (
                            f"processed probe_key_events_delta={probe_key_events_delta} "
                            f"expected_min={minimum_expected_key_events} total_key_events={key_events_seen}"
                        ),
                    }
                ],
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
    )

    write_command_log(ctx, command, text, "", exit_code)
    assertions = [
        ("process_exited_zero", exit_code == 0, f"exit={exit_code}"),
        ("prompt_sent", sent_prompt, f"sent_prompt={sent_prompt}"),
        ("background_agent_launched", snapshot["async_agent_result_seen"], json.dumps(snapshot, ensure_ascii=False)[:2000]),
        ("child_agent_request_seen", snapshot["child_request_seen"], json.dumps(snapshot, ensure_ascii=False)[:2000]),
        ("taskoutput_result_seen", snapshot["task_output_result_seen"], json.dumps(snapshot, ensure_ascii=False)[:2000]),
        ("input_probe_sent_while_child_running", sent_input_probe, f"probe={INPUT_PROBE_TEXT}"),
        (
            "input_probe_key_events_processed",
            probe_key_events_delta >= minimum_expected_key_events,
            (
                f"probe_key_events_delta={probe_key_events_delta} "
                f"expected_min={minimum_expected_key_events} total_key_events={key_events_seen} "
                f"before={probe_key_events_before} after={probe_key_events_after}"
            ),
        ),
        ("parent_final_marker_rendered", PARENT_MARKER in text, PARENT_MARKER),
    ]
    all_ok = all(passed for _, passed, _ in assertions)
    write_assertions(
        ctx,
        status="passed" if all_ok else "failed",
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
            "tui_events": str(tui_event_log_path),
            "input_responsive_evidence": str(evidence_path),
        },
    )

    return {
        "ok": all_ok,
        "fixture_root": str(ctx.root_dir),
        "exit_code": exit_code,
        "sent_prompt": sent_prompt,
        "sent_input_probe": sent_input_probe,
        "sent_quit": sent_quit,
        "key_events_seen": key_events_seen,
        "probe_key_events_before": probe_key_events_before,
        "probe_key_events_after": probe_key_events_after,
        "probe_key_events_delta": probe_key_events_delta,
        "minimum_expected_key_events": minimum_expected_key_events,
        "mock": {
            "request_count": snapshot["request_count"],
            "captured_task_id": snapshot["captured_task_id"],
            "async_agent_result_seen": snapshot["async_agent_result_seen"],
            "child_request_seen": snapshot["child_request_seen"],
            "task_output_result_seen": snapshot["task_output_result_seen"],
        },
        "artifacts": {
            "pty_output": str(text_path),
            "mock_requests": str(mock_path),
            "actions": str(actions_path),
            "tui_events": str(tui_event_log_path),
            "input_responsive_evidence": str(evidence_path),
            "assertions": str(ctx.artifacts_dir / "assertions.json"),
        },
    }


def main() -> int:
    result = run_probe()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
