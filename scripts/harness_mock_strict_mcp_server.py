#!/usr/bin/env python3
"""
最简严格 mock MCP stdio server for harness M3.5.

vs harness_mock_mcp_server.py 的差别:
  暴露一个 strict_tool_M3_5 工具,
  inputSchema 含 required: ["text"], 且 server 在 tools/call 处主动验:
    - 若 arguments 缺 'text' 字段 → 返 {isError: true,
        content: [{type:'text', text:'MISSING_REQUIRED_text_M3_5'}]}
    - 若 arguments 含 text='valid' → 正常返
  这样 mossen 端会沿 client.ts:3122 isError 路径抛 McpToolCallError, 在
  session log 的 tool_result 里 is_error=true 字面可见.

只实现 harness 测试需要的 method:
  - initialize / notifications/initialized
  - tools/list
  - tools/call (含 strict 验)
  - ping
"""

import json
import sys

PROTOCOL_VERSION = "2024-11-05"
SERVER_NAME = "harness-mock-strict-mcp"
SERVER_VERSION = "0.1.0"

STRICT_TOOL_NAME = "strict_tool_M3_5"
MISSING_MARKER = "MISSING_REQUIRED_text_M3_5"
SUCCESS_MARKER = "STRICT_OK_M3_5"


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
                "name": STRICT_TOOL_NAME,
                "description": "Strict tool that rejects missing 'text' arg",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "Required text payload",
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
    if name != STRICT_TOOL_NAME:
        respond(req_id, error={
            "code": -32601,
            "message": f"Unknown tool: {name}",
        })
        return

    # Server-side enforcement: missing OR empty 'text' -> isError true.
    # 注: mossen client 端会做 schema validation, 若 required 缺失会自动补
    # 空字符串后调用. 因此 server 也必须拒空 text 才能验证 schema 真生效.
    if "text" not in args or not isinstance(args.get("text"), str) or args.get("text") == "":
        respond(req_id, result={
            "content": [
                {"type": "text", "text": MISSING_MARKER},
            ],
            "isError": True,
        })
        return

    respond(req_id, result={
        "content": [
            {"type": "text", "text": f"{SUCCESS_MARKER}: {args['text']}"},
        ],
        "isError": False,
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
