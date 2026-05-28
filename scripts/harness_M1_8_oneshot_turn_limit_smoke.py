#!/usr/bin/env python3
"""M1.8 - oneshot --turn-limit reaches the live Rust dialogue loop."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log
from lib.mock_openai_provider import apply_mock_provider_env, mock_openai_provider


def parse_terminal(stdout: str) -> str | None:
    terminal: str | None = None
    for line in stdout.splitlines():
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(payload, dict) and payload.get("type") == "result":
            value = payload.get("terminal")
            if isinstance(value, str):
                terminal = value
    return terminal


def ok_assertion(name: str, ok: bool, **detail: Any) -> dict[str, Any]:
    return {"name": name, "ok": ok, **detail}


def main() -> int:
    ctx = make_fixture("M1.8_oneshot_turn_limit")
    ctx.env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    ctx.env["MOSSEN_START_BUILD"] = "never"
    if not (ROOT / "target" / "debug" / "mossen").exists():
        ctx.env["MOSSEN_START_BUILD"] = "auto"

    command = [
        str(ROOT / "scripts" / "start-mossen.sh"),
        "--oneshot",
        "R3_TEST_MARKER_TURNLIMIT use bash once",
        "--emit",
        "stream-json",
        "--access-policy",
        "unrestricted",
        "--instruments",
        "Bash",
        "--turn-limit",
        "1",
        "--cwd",
        str(ctx.root_dir),
    ]

    with mock_openai_provider(model="m1-8-turn-limit-model") as (base_url, provider):
        apply_mock_provider_env(
            ctx.env,
            base_url,
            model="m1-8-turn-limit-model",
            name="M1.8 Turn Limit Mock",
        )
        proc = subprocess.run(
            command,
            cwd=str(ctx.root_dir),
            env=ctx.env,
            text=True,
            capture_output=True,
            timeout=90,
            check=False,
        )
        provider_snapshot = provider.snapshot()

    terminal = parse_terminal(proc.stdout)
    (ctx.artifacts_dir / "session_log.jsonl").write_text(proc.stdout, encoding="utf-8")
    (ctx.artifacts_dir / "provider_snapshot.json").write_text(
        json.dumps(provider_snapshot, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    write_command_log(ctx, command, proc.stdout, proc.stderr, proc.returncode)

    assertions = [
        ok_assertion("process_exited_zero", proc.returncode == 0, exit_code=proc.returncode),
        ok_assertion(
            "terminal_uses_requested_turn_limit",
            terminal == "MaxTurns { turn_count: 1 }",
            terminal=terminal,
        ),
        ok_assertion(
            "provider_saw_single_request_before_limit",
            provider_snapshot["request_count"] == 1,
            request_count=provider_snapshot["request_count"],
        ),
        ok_assertion(
            "hardcoded_default_12_not_used",
            terminal != "MaxTurns { turn_count: 12 }",
            terminal=terminal,
        ),
    ]
    status = "passed" if all(item["ok"] for item in assertions) else "failed"
    write_assertions(
        ctx,
        status=status,
        assertions=assertions,
        extra_artifacts={"provider_snapshot": str(ctx.artifacts_dir / "provider_snapshot.json")},
    )
    print(json.dumps({"status": status, "terminal": terminal, "assertions": assertions}, indent=2))
    return 0 if status == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
