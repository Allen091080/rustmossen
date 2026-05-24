#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from wave_w144_stream_json_terminal_frontend_pty_smoke import run_terminal_emit_pty_smoke

TEST_ID = "W145_stream_json_terminal_log_isolation_smoke"


def main() -> int:
    result = run_terminal_emit_pty_smoke(TEST_ID)
    pty_output = Path(result["artifacts"]["pty_output"])
    assertions_path = Path(result["artifacts"]["assertions"])
    text = pty_output.read_text(encoding="utf-8", errors="replace")
    leaked_tokens = [
        " INFO mossen",
        " WARN mossen",
        " ERROR mossen",
        "logging initialized",
        "init_sequence:",
        "setup:",
        "cli_ok:",
        "cleanup:",
    ]
    leaks = [token for token in leaked_tokens if token in text]
    log_isolated = not leaks
    output_has_terminal_bytes = result["head_marker"] and result["tail_marker"]

    payload = json.loads(assertions_path.read_text(encoding="utf-8"))
    payload["assertions"].extend(
        [
            {
                "name": "terminal_emit_log_isolated",
                "expected": True,
                "actual": log_isolated,
                "passed": log_isolated,
                "evidence": ",".join(leaks) if leaks else "no tracing tokens in PTY output",
            },
            {
                "name": "terminal_emit_still_renders_content",
                "expected": True,
                "actual": output_has_terminal_bytes,
                "passed": output_has_terminal_bytes,
                "evidence": "head/tail marker present after log isolation",
            },
        ]
    )
    ok = bool(result["ok"] and log_isolated and output_has_terminal_bytes)
    payload["status"] = "passed" if ok else "failed"
    assertions_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False))

    result.update(
        {
            "ok": ok,
            "log_isolated": log_isolated,
            "log_leaks": leaks,
        }
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
