#!/usr/bin/env python3
"""Wave 0 — PERM-1 focused smoke.

Verifies utils/permissions/dangerousPatterns.ts after the Wave 0 split:
  * Network/exfil + cloud-write entries (gh / gh api / curl / wget / git /
    kubectl / aws / gcloud / gsutil) are present for ALL USER_TYPE values.
  * Anthropic-internal launchers (fa run / coo) remain gated behind
    USER_TYPE === 'ant'.

Implementation notes:
  * Pure Bun-runtime evaluation — no LLM, no real backend, no ~/.mossen
    write. Uses run-bun-featured.sh to import the module under three
    USER_TYPE values and serialise DANGEROUS_BASH_PATTERNS as JSON.
  * Designed to be CI-stable: independent of node_modules state because
    the imported file is plain TS with zero external imports.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

ALWAYS_ON = [
    "gh",
    "gh api",
    "curl",
    "wget",
    "git",
    "kubectl",
    "aws",
    "gcloud",
    "gsutil",
]
INTERNAL_ONLY = ["fa run", "coo"]


def evaluate(user_type: str | None) -> list[str]:
    """Import dangerousPatterns under the given USER_TYPE and return the
    resolved DANGEROUS_BASH_PATTERNS array."""
    snippet = (
        "import { DANGEROUS_BASH_PATTERNS } from "
        "'./utils/permissions/dangerousPatterns.js'; "
        "process.stdout.write(JSON.stringify(DANGEROUS_BASH_PATTERNS));"
    )
    env = {
        "PATH": os.environ.get(
            "PATH", "/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin"
        ),
        "HOME": os.environ.get("HOME", "/tmp"),
    }
    if user_type is not None:
        env["USER_TYPE"] = user_type
    proc = subprocess.run(
        ["bun", "-e", snippet],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"bun eval failed (USER_TYPE={user_type!r}): "
            f"rc={proc.returncode}\nstderr={proc.stderr!r}"
        )
    return json.loads(proc.stdout)


def main() -> int:
    cases: dict[str, dict[str, object]] = {}

    for label, ut in [("undefined", None), ("ant", "ant"), ("mossen", "mossen")]:
        patterns = evaluate(ut)
        missing_always_on = [p for p in ALWAYS_ON if p not in patterns]
        ant_only_present = [p for p in INTERNAL_ONLY if p in patterns]
        cases[label] = {
            "USER_TYPE": ut,
            "patterns_count": len(patterns),
            "missing_always_on": missing_always_on,
            "ant_only_present": ant_only_present,
        }

    failures: list[str] = []

    # Always-on entries must appear under all three USER_TYPE values.
    for label, info in cases.items():
        if info["missing_always_on"]:
            failures.append(
                f"USER_TYPE={label}: missing always-on entries "
                f"{info['missing_always_on']}"
            )

    # 'fa run' / 'coo' must appear ONLY under USER_TYPE=ant.
    if cases["undefined"]["ant_only_present"]:
        failures.append(
            "USER_TYPE=undefined: ant-only entries leaked: "
            f"{cases['undefined']['ant_only_present']}"
        )
    if cases["mossen"]["ant_only_present"]:
        failures.append(
            "USER_TYPE=mossen: ant-only entries leaked: "
            f"{cases['mossen']['ant_only_present']}"
        )
    if set(cases["ant"]["ant_only_present"]) != set(INTERNAL_ONLY):
        failures.append(
            "USER_TYPE=ant: ant-only entries incomplete: "
            f"have={cases['ant']['ant_only_present']}, want={INTERNAL_ONLY}"
        )

    report = {
        "name": "wave0_perm1_dangerous_patterns_smoke",
        "cases": cases,
        "failures": failures,
        "passed": 0 if failures else 1,
        "total": 1,
    }
    print(json.dumps(report, indent=2))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
