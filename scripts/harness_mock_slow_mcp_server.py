#!/usr/bin/env python3
"""
Slow mock MCP stdio server for harness M10.1 / M10.2.

JSON-RPC 2.0 over stdio. Exposes 2 tools:
  - slow_M10_1: sleeps SLOW_SLEEP_SECS (default 10) then returns SLOW_TAG
  - forever_M10_2: sleeps FOREVER_SLEEP_SECS (default 60) then returns
    (used to test timeout behavior; mossen 主进程在它返回前 timeout)

Sleep durations are env-overridable so tests can dial them up/down without
forking the file.
"""

import json
import os
import sys
import time

PROTOCOL_VERSION = "2024-11-05"
SERVER_NAME = "harness-mock-slow-mcp"
SERVER_VERSION = "0.1.0"

SLOW_TOOL_NAME = "slow_M10_1"
SLOW_TAG = "SLOW_TAG_FROM_MOCK_M10_1"
SLOW_SLEEP_SECS = int(os.environ.get("HARNESS_SLOW_SLEEP_SECS", "10"))

FOREVER_TOOL_NAME = "forever_M10_2"
FOREVER_TAG = "FOREVER_TAG_FROM_MOCK_M10_2"
FOREVER_SLEEP_SECS = int(os.environ.get("HARNESS_FOREVER_SLEEP_SECS", "60"))


def respond(req_id, result=None, error=None):
    payload = {"jsonrpc": "2.0", "id": req_id}
    if error is not None:
        payload["error"] = error
    else:
        payload["result"] = result
    sys.stdout.write(json.dumps(payload) + "\n")
    sys.stdout.flush()


def handle_initialize(req_id, params):
    respond(req_id, result={
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {"tools": {}},
        "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
    })


def handle_tools_list(req_id, params):
    respond(req_id, result={
        "tools": [
            {
                "name": SLOW_TOOL_NAME,
                "description": (
                    f"Sleeps {SLOW_SLEEP_SECS}s on the server side, then "
                    "returns a tag. Used to test long-running tool support."
                ),
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "note": {
                            "type": "string",
                            "description": "Free-form note echoed back (optional).",
                        },
                    },
                    "required": [],
                },
            },
            {
                "name": FOREVER_TOOL_NAME,
                "description": (
                    f"Sleeps {FOREVER_SLEEP_SECS}s server side. Used to test "
                    "tool timeout attribution — caller should treat as timeout."
                ),
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "note": {"type": "string"},
                    },
                    "required": [],
                },
            },
        ]
    })


def handle_tools_call(req_id, params):
    name = (params or {}).get("name")
    args = (params or {}).get("arguments") or {}
    note = args.get("note", "")

    if name == SLOW_TOOL_NAME:
        # Real wall-clock sleep so caller sees a real long-running tool.
        time.sleep(SLOW_SLEEP_SECS)
        respond(req_id, result={
            "content": [
                {"type": "text",
                 "text": f"{SLOW_TAG}: slept={SLOW_SLEEP_SECS}s note={note}"}
            ],
            "isError": False,
        })
        return

    if name == FOREVER_TOOL_NAME:
        time.sleep(FOREVER_SLEEP_SECS)
        respond(req_id, result={
            "content": [
                {"type": "text",
                 "text": f"{FOREVER_TAG}: slept={FOREVER_SLEEP_SECS}s note={note}"}
            ],
            "isError": False,
        })
        return

    respond(req_id, error={
        "code": -32601,
        "message": f"Unknown tool: {name}",
    })


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue

        method = req.get("method")
        req_id = req.get("id")
        params = req.get("params")

        if req_id is None:
            continue

        if method == "initialize":
            handle_initialize(req_id, params)
        elif method == "tools/list":
            handle_tools_list(req_id, params)
        elif method == "tools/call":
            handle_tools_call(req_id, params)
        elif method == "ping":
            respond(req_id, result={})
        else:
            respond(req_id, error={
                "code": -32601,
                "message": f"Method not found: {method}",
            })


if __name__ == "__main__":
    main()
