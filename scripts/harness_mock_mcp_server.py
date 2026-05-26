#!/usr/bin/env python3
"""
最简 mock MCP stdio server for harness M3.2.

实现 JSON-RPC 2.0 over stdio, 暴露一个 echo_M3_2 tool:
  - 接收 {"text": "...PAYLOAD..."}
  - 返回 content: [{type: "text", text: "ECHO_TAG: ...PAYLOAD..."}]

只实现 harness 测试需要的 method:
  - initialize
  - notifications/initialized (no response)
  - tools/list
  - tools/call
  - resources/list
  - resources/read

不实现: prompts, sampling 等高级特性。
"""

import json
import sys

PROTOCOL_VERSION = "2024-11-05"
SERVER_NAME = "harness-mock-mcp"
SERVER_VERSION = "0.1.0"

ECHO_TOOL_NAME = "echo_M3_2"
ECHO_TAG = "ECHO_TAG_FROM_MOCK_MCP"
RESOURCE_URI = "mcp://fixture/doc"
RESOURCE_BODY = "RESOURCE_BODY_M3"


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
        "capabilities": {
            "tools": {},
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION,
        },
    })


def handle_tools_list(req_id, params):
    respond(req_id, result={
        "tools": [
            {
                "name": ECHO_TOOL_NAME,
                "description": "Echo the input text with a fixed tag prefix",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "Text to echo back",
                        },
                    },
                    "required": ["text"],
                },
            }
        ]
    })


def handle_tools_call(req_id, params):
    name = (params or {}).get("name")
    args = (params or {}).get("arguments") or {}
    if name != ECHO_TOOL_NAME:
        respond(req_id, error={
            "code": -32601,
            "message": f"Unknown tool: {name}",
        })
        return
    text = args.get("text", "")
    respond(req_id, result={
        "content": [
            {"type": "text", "text": f"{ECHO_TAG}: {text}"}
        ],
        "isError": False,
    })


def handle_resources_list(req_id, params):
    respond(req_id, result={
        "resources": [
            {
                "uri": RESOURCE_URI,
                "name": "fixture-doc",
                "description": "Fixture MCP resource",
                "mimeType": "text/plain",
            }
        ]
    })


def handle_resources_read(req_id, params):
    uri = (params or {}).get("uri")
    if uri != RESOURCE_URI:
        respond(req_id, error={
            "code": -32602,
            "message": f"Unknown resource: {uri}",
        })
        return
    respond(req_id, result={
        "contents": [
            {
                "uri": RESOURCE_URI,
                "mimeType": "text/plain",
                "text": RESOURCE_BODY,
            }
        ]
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

        # Notifications (no id) — process但不响应
        if req_id is None:
            continue

        if method == "initialize":
            handle_initialize(req_id, params)
        elif method == "tools/list":
            handle_tools_list(req_id, params)
        elif method == "tools/call":
            handle_tools_call(req_id, params)
        elif method == "resources/list":
            handle_resources_list(req_id, params)
        elif method == "resources/read":
            handle_resources_read(req_id, params)
        elif method == "ping":
            respond(req_id, result={})
        else:
            respond(req_id, error={
                "code": -32601,
                "message": f"Method not found: {method}",
            })


if __name__ == "__main__":
    main()
