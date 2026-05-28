#!/usr/bin/env python3
"""Wave 0 — PERM-2 focused smoke (static-only).

Verifies that the if-block guarding findOverlyBroadBashPermissions /
findOverlyBroadPowerShellPermissions in setupToolPermissionContext no longer
gates on `process.env.USER_TYPE === 'internal'`, while the two surviving skip
conditions (MOSSEN_CODE_REMOTE / MOSSEN_CODE_ENTRYPOINT='local-agent') are
intact.

Why static-only (not runtime):
  * permissionSetup.ts transitively imports tools.ts → REPLTool/REPLTool.js,
    which is a deferred runtime-only dynamic import that cannot be resolved
    via `bun -e` against the source tree (works only inside the assembled
    runtime via run-mossen.sh). Importing the file under bun -e fails with
    "Cannot find module './tools/REPLTool/REPLTool.js'".
  * The PERM-2 change is purely structural ("delete one clause from one
    if-condition"); a static structural assertion is sufficient to confirm
    correctness. The downstream behaviour (Bash(*) detection running for
    USER_TYPE=undefined users) is exercised by setup integration paths
    inside the existing TUI smoke harness, not by an isolated runtime eval.
  * Recorded here per Allen's directive to not fake runtime passes.

No LLM, no real backend, no ~/.mossen write. Pure file read + regex.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SETUP_PATH = ROOT / "utils" / "permissions" / "permissionSetup.ts"


def static_assertion() -> dict[str, object]:
    text = SETUP_PATH.read_text(encoding="utf-8")

    findings: dict[str, object] = {
        "block_found": False,
        "user_type_internal_in_overlybroad_block": False,
        "remote_skip_present": False,
        "local_agent_skip_present": False,
        "finder_calls_present": False,
    }

    # Locate the overlyBroadBashPermissions block.
    block_re = re.compile(
        r"let overlyBroadBashPermissions: DangerousPermissionInfo\[\] = \[\][\s\S]{0,800}?\n  \}",
        re.MULTILINE,
    )
    m = block_re.search(text)
    if m is None:
        return findings
    block = m.group(0)
    findings["block_found"] = True

    if "process.env.USER_TYPE === 'internal'" in block:
        findings["user_type_internal_in_overlybroad_block"] = True
    if "MOSSEN_CODE_REMOTE" in block:
        findings["remote_skip_present"] = True
    if "MOSSEN_CODE_ENTRYPOINT" in block and "'local-agent'" in block:
        findings["local_agent_skip_present"] = True
    if (
        "findOverlyBroadBashPermissions(" in block
        and "findOverlyBroadPowerShellPermissions(" in block
    ):
        findings["finder_calls_present"] = True

    return findings


def main() -> int:
    failures: list[str] = []
    findings = static_assertion()

    if not findings.get("block_found"):
        failures.append("overlyBroadBashPermissions block not found in permissionSetup.ts")
    else:
        if findings["user_type_internal_in_overlybroad_block"]:
            failures.append(
                "process.env.USER_TYPE === 'internal' still present inside overlyBroad block "
                "(PERM-2 not applied)"
            )
        if not findings["remote_skip_present"]:
            failures.append(
                "MOSSEN_CODE_REMOTE skip condition missing — PERM-2 must preserve this gate"
            )
        if not findings["local_agent_skip_present"]:
            failures.append(
                "MOSSEN_CODE_ENTRYPOINT='local-agent' skip condition missing — "
                "PERM-2 must preserve this gate"
            )
        if not findings["finder_calls_present"]:
            failures.append(
                "findOverlyBroadBashPermissions/findOverlyBroadPowerShellPermissions "
                "calls missing from block"
            )

    report = {
        "name": "wave0_perm2_overly_broad_smoke",
        "mode": "static-only",
        "mode_reason": (
            "permissionSetup.ts transitively imports REPLTool/REPLTool.js "
            "(deferred dynamic import unresolvable via bun -e against source). "
            "Runtime exercise belongs in TUI/setup integration smoke."
        ),
        "static_findings": findings,
        "failures": failures,
        "passed": 0 if failures else 1,
        "total": 1,
    }
    print(json.dumps(report, indent=2))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
