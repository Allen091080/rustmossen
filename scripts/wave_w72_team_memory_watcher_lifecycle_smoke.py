#!/usr/bin/env python3
"""
W72 - team memory watcher lifecycle smoke.

This is a static guard for the Rust CLI wiring:

1. Subcommands must bypass session background services.
2. Normal session routes must start the team-memory watcher before dispatching
   oneshot/stdin/input-file/interactive work.
3. run() must stop session background services after route_command returns so
   error paths also flush/stop the watcher before CLI cleanup.
4. The helper functions must call the mossen-agent team_memory_sync watcher API.
5. run_all_smoke.sh must register this smoke.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAIN_RS = ROOT / "crates" / "mossen-cli" / "src" / "main.rs"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def must_find(src: str, needle: str, failures: list[str], message: str) -> int:
    idx = src.find(needle)
    if idx < 0:
        fail(failures, message)
    return idx


def main() -> int:
    failures: list[str] = []
    src = MAIN_RS.read_text(encoding="utf-8")
    run_all = RUN_ALL.read_text(encoding="utf-8")

    route_idx = must_find(
        src,
        "let result = route_command(cli, state.clone(), shutdown).await;",
        failures,
        "run() must route the CLI command and keep the result for cleanup",
    )
    stop_after_route_idx = must_find(
        src,
        "stop_session_background_services().await;",
        failures,
        "run() must stop session background services after routing",
    )
    if route_idx >= 0 and stop_after_route_idx >= 0 and stop_after_route_idx < route_idx:
        fail(failures, "stop_session_background_services() must run after route_command()")

    subcmd_idx = must_find(
        src,
        "if let Some(subcmd) = cli.command",
        failures,
        "route_command() must keep subcommand short-circuit",
    )
    start_idx = must_find(
        src,
        "start_session_background_services().await;",
        failures,
        "route_command() must start session background services for session modes",
    )
    registries_idx = must_find(
        src,
        "let directives = DirectiveRegistry::new();",
        failures,
        "route_command() must still initialize directives after lifecycle startup",
    )
    if subcmd_idx >= 0 and start_idx >= 0 and start_idx < subcmd_idx:
        fail(failures, "subcommands must bypass start_session_background_services()")
    if start_idx >= 0 and registries_idx >= 0 and start_idx > registries_idx:
        fail(failures, "session background services should start before session registries dispatch")

    if (
        "async fn start_session_background_services()" not in src
        or "mossen_agent::services::team_memory_sync::start_team_memory_watcher().await" not in src
    ):
        fail(failures, "start_session_background_services() must call team_memory_sync start")
    if (
        "async fn stop_session_background_services()" not in src
        or "mossen_agent::services::team_memory_sync::stop_team_memory_watcher().await" not in src
    ):
        fail(failures, "stop_session_background_services() must call team_memory_sync stop")

    if "wave_w72_team_memory_watcher_lifecycle_smoke" not in run_all:
        fail(failures, "run_all_smoke.sh must register W72")

    print("=== W72 team memory watcher lifecycle smoke ===")
    print(f"main.rs: {MAIN_RS.relative_to(ROOT)}")
    print(f"run_all: {RUN_ALL.relative_to(ROOT)}")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W72 team-memory watcher lifecycle is wired "
        "(subcommands bypass, session routes start, run cleanup stops, "
        "helpers call mossen-agent watcher API, run_all registers smoke)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
