#!/usr/bin/env python3
"""
M0.3 - current Rust command inventory matrix.

The matrix is generated from `mossen_commands::all_directives()` in the current
Rust registry. This harness only invokes that Rust check, validates the emitted
JSON schema, and records the artifact used by later command-coverage gates.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log  # noqa: E402


MATRIX_PATH = ROOT / "harness_slash_command_matrix.json"
REAL_HOME = Path.home()
ALLOWED_CATEGORIES = {
    "no_side_effect",
    "writes_config",
    "external_service",
    "high_risk_tool",
    "temporarily_unsupported",
}
MUST_HAVE = {
    "help",
    "clear",
    "compact",
    "context",
    "model",
    "mcp",
    "memory",
    "status",
    "permissions",
    "skills",
    "plugin",
    "lang",
    "resume",
    "agents",
}


def run_command(command: list[str], env: dict[str, str]) -> dict[str, Any]:
    proc = subprocess.run(
        command,
        cwd=str(ROOT),
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=180,
    )
    return {
        "command": command,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "exit_code": proc.returncode,
    }


def load_matrix() -> dict[str, Any]:
    return json.loads(MATRIX_PATH.read_text(encoding="utf-8"))


def case_generate_matrix(ctx: Any) -> dict[str, Any]:
    env = ctx.env.copy()
    env.setdefault("CARGO_HOME", str(REAL_HOME / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(REAL_HOME / ".rustup"))
    env["MOSSEN_COMMAND_MATRIX_JSON"] = str(MATRIX_PATH)
    result = run_command(
        [
            "cargo",
            "test",
            "-p",
            "mossen-commands",
            "command_inventory_matrix_covers_current_personal_registry",
            "--",
            "--nocapture",
        ],
        env,
    )
    write_command_log(
        ctx,
        result["command"],
        result["stdout"],
        result["stderr"],
        result["exit_code"],
    )
    matrix_exists = MATRIX_PATH.exists()
    matrix_total = None
    if matrix_exists:
        matrix_total = load_matrix().get("total")
    return {
        "name": "generate_matrix_from_current_rust_registry",
        "ok": result["exit_code"] == 0 and matrix_exists and (matrix_total or 0) >= 50,
        "exit_code": result["exit_code"],
        "matrix_path": str(MATRIX_PATH),
        "matrix_total": matrix_total,
    }


def case_schema_and_categories() -> dict[str, Any]:
    try:
        matrix = load_matrix()
    except Exception as error:  # noqa: BLE001
        return {
            "name": "schema_and_categories",
            "ok": False,
            "error": str(error),
        }

    entries = matrix.get("entries", [])
    categories = {entry.get("category") for entry in entries}
    missing_fields = [
        entry.get("command", "<unknown>")
        for entry in entries
        if not all(
            key in entry
            for key in (
                "command",
                "visible",
                "category",
                "side_effect",
                "test_mode",
                "expected",
                "script",
            )
        )
    ]
    unknown_categories = sorted(str(category) for category in categories - ALLOWED_CATEGORIES)
    return {
        "name": "schema_and_categories",
        "ok": (
            matrix.get("total") == len(entries)
            and len(entries) >= 50
            and not missing_fields
            and not unknown_categories
        ),
        "total": matrix.get("total"),
        "entry_count": len(entries),
        "categories": sorted(str(category) for category in categories),
        "missing_fields": missing_fields[:20],
        "unknown_categories": unknown_categories,
    }


def case_known_core_commands_present() -> dict[str, Any]:
    matrix = load_matrix()
    names = {entry["command"] for entry in matrix["entries"]}
    missing = sorted(MUST_HAVE - names)
    return {
        "name": "known_core_commands_present",
        "ok": not missing,
        "must_have_count": len(MUST_HAVE),
        "missing": missing,
        "matched_count": len(MUST_HAVE) - len(missing),
    }


def case_visible_personal_surface_is_populated() -> dict[str, Any]:
    matrix = load_matrix()
    entries = matrix["entries"]
    visible_entries = [entry for entry in entries if entry.get("visible")]
    unfinished_terms = (
        "placeholder",
        "stub",
        "not implemented",
        "unimplemented",
        "not wired",
        "phase 5 tui",
        "hosted workflow",
        "direct-connect",
        "ssh remote",
        "remote attach",
        "team memory sync",
    )
    leaked = [
        entry["command"]
        for entry in visible_entries
        if any(term in entry.get("description", "").lower() for term in unfinished_terms)
    ]
    return {
        "name": "visible_personal_surface_is_populated",
        "ok": len(visible_entries) >= 50 and not leaked,
        "visible_count": len(visible_entries),
        "leaked_unfinished_descriptions": leaked,
    }


def main() -> int:
    ctx = make_fixture("M0.3")

    results = [
        case_generate_matrix(ctx),
        case_schema_and_categories(),
        case_known_core_commands_present(),
        case_visible_personal_surface_is_populated(),
    ]
    status = "passed" if all(result.get("ok") for result in results) else "failed"

    write_assertions(
        ctx,
        status=status,
        assertions=[
            {
                "name": result["name"],
                "expected": True,
                "actual": result.get("ok"),
                "passed": result.get("ok"),
            }
            for result in results
        ],
        extra_artifacts={"matrix_json": str(MATRIX_PATH)},
    )

    summary = {
        "test_id": "M0.3_command_inventory_current_rust",
        "status": status,
        "results": results,
        "passed": sum(1 for result in results if result.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "matrix_json": str(MATRIX_PATH),
        "design_note": "M0.3 generates the slash-command matrix from the current Rust registry.",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if status == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
