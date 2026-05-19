#!/usr/bin/env python3
"""W41 — CLI capability matrix smoke.

Purpose:
  Verify the main CLI/Core capability surface in one place before exposing
  more stream-json wrappers to external clients such as Workbench.

Scope:
  - Skills: list, invoke, reload, sources, agent-loop injection, error isolation
  - MCP: register, call, scope/failure isolation, schema validation
  - Plugins: install/list, command trigger, reload/disable, failure isolation

This script intentionally exercises existing harnesses instead of duplicating
their internals. It is a capability gate, not a UI smoke and not a protocol
wrapper implementation.
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class CapabilityHarness:
    group: str
    capability: str
    script: str


HARNESSES: tuple[CapabilityHarness, ...] = (
    CapabilityHarness("skill", "list", "scripts/harness_M6_1_skill_list_smoke.py"),
    CapabilityHarness("skill", "invoke", "scripts/harness_M6_2_skill_invoke_smoke.py"),
    CapabilityHarness("skill", "reload", "scripts/harness_M6_3_skill_reload_smoke.py"),
    CapabilityHarness("skill", "sources", "scripts/harness_M6_4_skill_sources_smoke.py"),
    CapabilityHarness("skill", "agent_loop_injection", "scripts/harness_M6_5_skill_inject_agent_loop_smoke.py"),
    CapabilityHarness("skill", "error_isolation", "scripts/harness_M6_6_skill_error_isolation_smoke.py"),
    CapabilityHarness("mcp", "register", "scripts/harness_M3_1_mcp_register_smoke.py"),
    CapabilityHarness("mcp", "call", "scripts/harness_M3_2_mcp_call_smoke.py"),
    CapabilityHarness("mcp", "scope_and_failed_server", "scripts/harness_M3_4_mcp_scope_failed_server_smoke.py"),
    CapabilityHarness("mcp", "schema_validation", "scripts/harness_M3_5_mcp_tool_schema_validation_smoke.py"),
    CapabilityHarness("plugin", "install_and_list", "scripts/harness_M7_1_plugin_install_list_smoke.py"),
    CapabilityHarness("plugin", "command_trigger", "scripts/harness_M7_2_plugin_command_trigger_smoke.py"),
    CapabilityHarness("plugin", "reload_and_disable", "scripts/harness_M7_3_plugin_reload_disable_smoke.py"),
    CapabilityHarness("plugin", "failure_isolation", "scripts/harness_M7_4_plugin_failure_isolation_smoke.py"),
)


def load_report(stdout: str) -> dict[str, Any] | None:
    try:
        parsed = json.loads(stdout)
    except json.JSONDecodeError:
        return None
    return parsed if isinstance(parsed, dict) else None


def run_harness(harness: CapabilityHarness) -> dict[str, Any]:
    started = time.perf_counter()
    proc = subprocess.run(
        ["python3", harness.script],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
        check=False,
    )
    elapsed_ms = round((time.perf_counter() - started) * 1000)
    report = load_report(proc.stdout)
    passed = report.get("passed") if report else None
    total = report.get("total") if report else None
    ok = proc.returncode == 0 and report is not None and passed == total
    return {
        "group": harness.group,
        "capability": harness.capability,
        "script": harness.script,
        "ok": ok,
        "exit_code": proc.returncode,
        "passed": passed,
        "total": total,
        "elapsed_ms": elapsed_ms,
        "design_note": report.get("design_note") if report else None,
        "stdout_excerpt": proc.stdout[:1200],
        "stderr_excerpt": proc.stderr[:1200],
    }


def main() -> int:
    print("=== W41 CLI capability matrix smoke ===")
    results = [run_harness(h) for h in HARNESSES]

    by_group: dict[str, list[dict[str, Any]]] = {}
    for result in results:
        by_group.setdefault(str(result["group"]), []).append(result)

    for group, items in by_group.items():
        ok_count = sum(1 for item in items if item["ok"])
        print(f"\n[{group}] {ok_count}/{len(items)}")
        for item in items:
            mark = "PASS" if item["ok"] else "FAIL"
            passed = item["passed"]
            total = item["total"]
            count = f"{passed}/{total}" if passed is not None and total is not None else "?"
            print(f"  {mark} {item['capability']} ({count}, {item['elapsed_ms']}ms)")

    failures = [r for r in results if not r["ok"]]
    if failures:
        print("\n=== FAIL DETAILS ===", file=sys.stderr)
        for failure in failures:
            print(
                f"- {failure['group']}.{failure['capability']} via {failure['script']} "
                f"(exit={failure['exit_code']}, passed={failure['passed']}, total={failure['total']})",
                file=sys.stderr,
            )
            if failure["stderr_excerpt"]:
                print(f"  stderr: {failure['stderr_excerpt']}", file=sys.stderr)
            else:
                print(f"  stdout: {failure['stdout_excerpt']}", file=sys.stderr)
        return 1

    total_assertions = sum(int(r["total"] or 0) for r in results)
    total_elapsed = sum(int(r["elapsed_ms"] or 0) for r in results)
    print(
        "\nPASS: CLI capability matrix ✓ "
        f"({len(results)} harnesses, {total_assertions} top-level cases, {total_elapsed}ms)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
