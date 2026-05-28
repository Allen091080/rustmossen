#!/usr/bin/env python3
"""Create or validate manual real-terminal release bake evidence."""

from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

DEFAULT_DIR = Path("/tmp/mossen-release-readiness/manual-tty-bake")
DEFAULT_EVIDENCE = DEFAULT_DIR / "evidence.json"
DEFAULT_TEMPLATE = DEFAULT_DIR / "evidence.template.json"
REQUIRED_CHECKS = (
    "input_responsive",
    "selection_copy_works",
    "scroll_returns_to_bottom",
    "subagent_completion_feedback",
    "ctrl_c_safe",
    "no_panic_or_deadlock",
    "no_render_tearing",
)


def template_payload(minimum_minutes: float) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "ok": False,
        "started_at": "",
        "ended_at": "",
        "duration_minutes": minimum_minutes,
        "terminal_app": "",
        "mossen_command": "./target/release/mossen",
        "profile": "",
        "checks": {name: False for name in REQUIRED_CHECKS},
        "notes": [
            "Fill this after a real terminal bake. Do not include API keys or secrets.",
            "selection_copy_works must be verified with the host terminal selection/copy path.",
        ],
    }


def load_json(path: Path) -> dict[str, Any] | None:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return payload if isinstance(payload, dict) else None


def validate_payload(payload: dict[str, Any], minimum_minutes: float) -> tuple[bool, list[str]]:
    errors: list[str] = []
    if payload.get("ok") is not True:
        errors.append("ok must be true")
    duration = float(payload.get("duration_minutes") or 0)
    if duration < minimum_minutes:
        errors.append(f"duration_minutes must be at least {minimum_minutes:g}")
    checks = payload.get("checks")
    if not isinstance(checks, dict):
        errors.append("checks must be an object")
        checks = {}
    for name in REQUIRED_CHECKS:
        if checks.get(name) is not True:
            errors.append(f"check {name!r} must be true")
    for field in ("terminal_app", "mossen_command", "started_at", "ended_at"):
        if not str(payload.get(field) or "").strip():
            errors.append(f"{field} must be recorded")
    return not errors, errors


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def prompt_text(label: str, default: str = "") -> str:
    suffix = f" [{default}]" if default else ""
    value = input(f"{label}{suffix}: ").strip()
    return value or default


def prompt_yes_no(label: str) -> bool:
    value = input(f"{label} [y/N]: ").strip().lower()
    return value in {"y", "yes"}


def require_real_tty() -> tuple[bool, str]:
    if not sys.stdin.isatty() or not sys.stdout.isatty():
        return False, "--record must be run from a real interactive TTY"
    return True, "interactive TTY detected"


def record_interactive_evidence(path: Path, minimum_minutes: float) -> tuple[bool, dict[str, Any]]:
    ok, reason = require_real_tty()
    if not ok:
        return False, {"ok": False, "status": "failed", "error": reason}

    print("Record manual TTY bake evidence. Do not enter API keys or secrets.")
    if not prompt_yes_no(
        f"Did you run Mossen in a real host terminal for at least {minimum_minutes:g} minutes?"
    ):
        return False, {"ok": False, "status": "cancelled", "error": "manual bake was not confirmed"}

    terminal_default = os.environ.get("TERM_PROGRAM") or os.environ.get("TERM") or ""
    payload = template_payload(minimum_minutes)
    payload["recorded_at"] = now_iso()
    payload["started_at"] = prompt_text("Bake started at ISO timestamp")
    payload["ended_at"] = prompt_text("Bake ended at ISO timestamp")
    payload["duration_minutes"] = float(prompt_text("Duration minutes", str(int(minimum_minutes))))
    payload["terminal_app"] = prompt_text("Terminal app", terminal_default)
    payload["mossen_command"] = prompt_text("Mossen command", "./target/release/mossen")
    payload["profile"] = prompt_text("Profile or provider used (no secrets)")
    payload["checks"] = {}
    for check in REQUIRED_CHECKS:
        payload["checks"][check] = prompt_yes_no(f"Check passed: {check}")
    payload["ok"] = all(payload["checks"].values())

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    valid, errors = validate_payload(payload, minimum_minutes)
    return valid, {
        "ok": valid,
        "status": "passed" if valid else "failed",
        "evidence": str(path),
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, default=DEFAULT_EVIDENCE)
    parser.add_argument("--template", type=Path, default=DEFAULT_TEMPLATE)
    parser.add_argument("--minimum-minutes", type=float, default=30)
    parser.add_argument("--write-template", action="store_true")
    parser.add_argument(
        "--record",
        action="store_true",
        help="Interactively record manual TTY bake evidence. Refuses non-TTY stdin/stdout.",
    )
    args = parser.parse_args()

    if args.record:
        ok, result = record_interactive_evidence(args.evidence, args.minimum_minutes)
        print(json.dumps(result, indent=2))
        return 0 if ok else 1

    if args.write_template:
        args.template.parent.mkdir(parents=True, exist_ok=True)
        args.template.write_text(
            json.dumps(template_payload(args.minimum_minutes), indent=2, ensure_ascii=False),
            encoding="utf-8",
        )
        print(json.dumps({"ok": True, "template": str(args.template)}, indent=2))
        return 0

    payload = load_json(args.evidence)
    if payload is None:
        print(
            json.dumps(
                {
                    "ok": False,
                    "status": "missing",
                    "evidence": str(args.evidence),
                    "hint": "record evidence from a real TTY with: python3 scripts/release_manual_tty_bake.py --record",
                },
                indent=2,
            )
        )
        return 1
    ok, errors = validate_payload(payload, args.minimum_minutes)
    print(
        json.dumps(
            {"ok": ok, "status": "passed" if ok else "failed", "errors": errors},
            indent=2,
        )
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
