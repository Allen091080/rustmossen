#!/usr/bin/env python3
"""
M10.6 - Parallel async Agents -> TaskOutput e2e.

This catches the regression seen in real use where many Agent calls in one
tool batch all returned the same visible task id (`agent-0`), so the parent
could not retrieve their output and fell back to doing the work itself.
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
PARENT_MARKER = "PARENT_OK_M10_6"
CHILD_MARKERS = ["PARALLEL_CHILD_A_M10_6", "PARALLEL_CHILD_B_M10_6", "PARALLEL_CHILD_C_M10_6"]


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
        self.task_ids: list[str] = []
        self.child_markers_seen: set[str] = set()
        self.task_output_markers_seen: set[str] = set()
        self.async_agent_result_seen = False

    def record(self, path: str, body: bytes) -> int:
        try:
            parsed = json.loads(body.decode("utf-8")) if body else {}
        except json.JSONDecodeError:
            parsed = {"_decode_error": body.decode("utf-8", "replace")}

        with self.lock:
            self.requests.append({"path": path, "body": parsed})
            for text in iter_strings(parsed):
                for marker in CHILD_MARKERS:
                    if "Mossen sub-agent launched by a parent session" in text and marker in text:
                        self.child_markers_seen.add(marker)
                    if marker in text and '"retrieval_status"' in text:
                        self.task_output_markers_seen.add(marker)
                if "async_launched" in text:
                    self.async_agent_result_seen = True
                    maybe_json = parse_json_string(text)
                    if isinstance(maybe_json, dict) and isinstance(maybe_json.get("task_id"), str):
                        self._add_task_id(maybe_json["task_id"])
                    else:
                        for match in re.finditer(r'"task_id"\s*:\s*"([^"]+)"', text):
                            self._add_task_id(match.group(1))
            return len(self.requests)

    def _add_task_id(self, task_id: str) -> None:
        if task_id not in self.task_ids:
            self.task_ids.append(task_id)

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "request_count": len(self.requests),
                "paths": [req["path"] for req in self.requests],
                "task_ids": list(self.task_ids),
                "unique_task_ids": sorted(set(self.task_ids)),
                "child_markers_seen": sorted(self.child_markers_seen),
                "task_output_markers_seen": sorted(self.task_output_markers_seen),
                "async_agent_result_seen": self.async_agent_result_seen,
                "requests": self.requests,
            }


def write_sse(wfile, payload: dict[str, Any]) -> None:
    wfile.write(f"data: {json.dumps(payload)}\n\n".encode("utf-8"))
    wfile.flush()


def write_tool_calls(wfile, call_specs: list[tuple[str, str, dict[str, Any]]]) -> None:
    write_sse(
        wfile,
        {
            "id": "m10-6-tools",
            "object": "chat.completion.chunk",
            "choices": [
                {
                    "index": 0,
                    "delta": {
                        "tool_calls": [
                            {
                                "index": index,
                                "id": call_id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": json.dumps(args),
                                },
                            }
                            for index, (call_id, name, args) in enumerate(call_specs)
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
            "id": "m10-6-tools",
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}],
            "usage": {"prompt_tokens": 20, "completion_tokens": 8},
        },
    )
    wfile.write(b"data: [DONE]\n\n")
    wfile.flush()


def write_text_final(wfile, text: str) -> None:
    write_sse(
        wfile,
        {
            "id": "m10-6-final",
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": None}],
        },
    )
    write_sse(
        wfile,
        {
            "id": "m10-6-final",
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 20, "completion_tokens": 8},
        },
    )
    wfile.write(b"data: [DONE]\n\n")
    wfile.flush()


def request_contains_child_prompt(body: dict[str, Any]) -> str | None:
    for text in iter_strings(body):
        if "Mossen sub-agent launched by a parent session" not in text:
            continue
        for marker in CHILD_MARKERS:
            if marker in text:
                return marker
    return None


def make_handler(state: MockOpenAIState):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            payload = json.dumps(
                {"object": "list", "data": [{"id": "m10-parallel-agent-model", "object": "model"}]}
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
            current_body = snapshot["requests"][-1]["body"]
            child_marker = request_contains_child_prompt(current_body)
            if not self.path.endswith("/chat/completions"):
                self.send_response(404)
                self.end_headers()
                return

            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            try:
                if child_marker:
                    write_text_final(self.wfile, f"{child_marker}: child completed.")
                elif request_index == 1:
                    write_tool_calls(
                        self.wfile,
                        [
                            (
                                f"call_m10_6_agent_{index}",
                                AGENT_TOOL_NAME,
                                {
                                    "description": f"parallel {index}",
                                    "prompt": f"Return {marker}.",
                                    "run_in_background": True,
                                },
                            )
                            for index, marker in enumerate(CHILD_MARKERS, start=1)
                        ],
                    )
                elif len(snapshot["unique_task_ids"]) == len(CHILD_MARKERS) and not snapshot[
                    "task_output_markers_seen"
                ]:
                    write_tool_calls(
                        self.wfile,
                        [
                            (
                                f"call_m10_6_task_output_{index}",
                                TASK_OUTPUT_TOOL_NAME,
                                {"task_id": task_id, "block": True, "timeout": 10000},
                            )
                            for index, task_id in enumerate(snapshot["task_ids"], start=1)
                        ],
                    )
                elif len(snapshot["task_output_markers_seen"]) == len(CHILD_MARKERS):
                    write_text_final(self.wfile, f"{PARENT_MARKER}: all parallel outputs retrieved.")
                else:
                    write_text_final(self.wfile, "PARENT_MISSING_PARALLEL_AGENT_OUTPUT_M10_6")
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


def case_parallel_agents_taskoutput_completes() -> dict:
    ctx = make_fixture("M10.6")
    env = dict(ctx.env)
    env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    env.update(
        {
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_MODEL": "m10-parallel-agent-model",
            "MOSSEN_CODE_CUSTOM_NAME": "M10 Parallel Agent Mock",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-m10-parallel-local-fake",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS": "30",
            "MOSSEN_CODE_CUSTOM_REQUEST_TIMEOUT_SECS": "30",
        }
    )
    prompt = f"Launch three background Agents, retrieve each with TaskOutput, then answer {PARENT_MARKER}."

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

    unique_task_ids = server_snapshot["unique_task_ids"]
    unique_visible_prefixes = {"-".join(task_id.split("-")[:2]) for task_id in unique_task_ids}
    ok = (
        proc.returncode == 0
        and PARENT_MARKER in proc.stdout
        and server_snapshot["async_agent_result_seen"]
        and len(unique_task_ids) == len(CHILD_MARKERS)
        and len(unique_visible_prefixes) == len(CHILD_MARKERS)
        and set(server_snapshot["child_markers_seen"]) == set(CHILD_MARKERS)
        and set(server_snapshot["task_output_markers_seen"]) == set(CHILD_MARKERS)
    )

    return {
        "name": "parallel_agents_taskoutput_completes",
        "ok": ok,
        "exit_code": proc.returncode,
        "async_agent_result_seen": server_snapshot["async_agent_result_seen"],
        "task_ids": server_snapshot["task_ids"],
        "unique_task_ids": unique_task_ids,
        "unique_visible_prefixes": sorted(unique_visible_prefixes),
        "child_markers_seen": server_snapshot["child_markers_seen"],
        "task_output_markers_seen": server_snapshot["task_output_markers_seen"],
        "parent_marker_in_stdout": PARENT_MARKER in proc.stdout,
        "model_request_count": server_snapshot["request_count"],
        "model_request_paths": server_snapshot["paths"],
        "stdout_excerpt": proc.stdout[:800],
        "stderr_excerpt": proc.stderr[:800],
        "fixture_root": str(ctx.root_dir),
        "model_requests": str(requests_path),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_parallel_agents_taskoutput_completes()
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
                    f"task_ids={r.get('unique_task_ids')} "
                    f"prefixes={r.get('unique_visible_prefixes')} "
                    f"child_markers={r.get('child_markers_seen')} "
                    f"task_output_markers={r.get('task_output_markers_seen')} "
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
            "M10.6 covers same-turn parallel Agent launches plus TaskOutput retrieval "
            "for every returned task id."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
