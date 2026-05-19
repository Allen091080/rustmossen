"""
mock_http_capture — 共享 mock HTTP server, 给 R1/R4/R7 用.

G2-2a 抽出 (G0-5 测试矩阵设计建议):
  - 原 R1/R4 各有 ~90 行 boilerplate; R7 还要再写一份.
  - 抽这个 lib 后, 三个 R-test 各自只需描述 path filter, 不再重复 server 代码.

API:
  port = alloc_port()
  server = MockCaptureServer.start(port=port)         # 启动后台 thread
  ...让被测进程跑...
  server.stop()
  reqs = server.received                              # list[dict]
  gb_reqs = server.filter(lambda r: r["path"].startswith("/api/features"))

每条 received_request 字段:
  method        str        "GET" / "POST" / etc.
  path          str        e.g. "/api/features/abc"
  host_header   str
  content_length int
  body_excerpt  str        前 200 字节, utf-8 lossy
  timestamp     float
"""

from __future__ import annotations

import socket
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from typing import Callable


def alloc_port() -> int:
    """让 OS 分配一个空闲端口."""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


class _CaptureHandler(BaseHTTPRequestHandler):
    """记录每个收到的请求, 永远返回 200."""

    received_requests: list[dict] = []
    response_body: bytes = b'{"ok":true}'

    def _record(self, method: str) -> None:
        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length) if length > 0 else b""
        type(self).received_requests.append({
            "method": method,
            "path": self.path,
            "host_header": self.headers.get("Host", ""),
            "content_length": length,
            "body_excerpt": body[:200].decode("utf-8", errors="replace"),
            "timestamp": time.time(),
        })
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(type(self).response_body)

    def do_GET(self) -> None:  # noqa: N802
        self._record("GET")

    def do_POST(self) -> None:  # noqa: N802
        self._record("POST")

    def do_PUT(self) -> None:  # noqa: N802
        self._record("PUT")

    def do_DELETE(self) -> None:  # noqa: N802
        self._record("DELETE")

    def log_message(self, format, *args):  # noqa: A002
        # 静音 stderr (不污染 fixture 输出)
        pass


class MockCaptureServer:
    """持续记录 HTTP 请求的 mock server. 不可重入, start 只能调一次."""

    def __init__(self, port: int, response_body: bytes = b'{"ok":true}'):
        self.port = port
        self._handler_class = type(
            f"_CaptureHandler_{port}",
            (_CaptureHandler,),
            {"received_requests": [], "response_body": response_body},
        )
        self._server: HTTPServer | None = None
        self._thread: threading.Thread | None = None

    @classmethod
    def start(cls, port: int | None = None, response_body: bytes = b'{"ok":true}') -> "MockCaptureServer":
        """便捷启动 (alloc_port 默认)."""
        if port is None:
            port = alloc_port()
        s = cls(port, response_body)
        s._start_internal()
        return s

    def _start_internal(self) -> None:
        if self._server is not None:
            raise RuntimeError("already started")
        self._server = HTTPServer(("127.0.0.1", self.port), self._handler_class)
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        if self._server is None:
            return
        self._server.shutdown()
        self._server.server_close()
        if self._thread is not None:
            self._thread.join(timeout=5)
        self._server = None
        self._thread = None

    @property
    def received(self) -> list[dict]:
        return list(self._handler_class.received_requests)

    def filter(self, predicate: Callable[[dict], bool]) -> list[dict]:
        return [r for r in self.received if predicate(r)]

    def reset(self) -> None:
        """清空记录, 不停 server."""
        self._handler_class.received_requests.clear()

    def __enter__(self) -> "MockCaptureServer":
        return self

    def __exit__(self, *args) -> None:
        self.stop()
