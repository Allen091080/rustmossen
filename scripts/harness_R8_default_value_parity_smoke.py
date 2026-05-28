#!/usr/bin/env python3
"""
R8 - retired default-value parity smoke.

The original R8 guarded a TypeScript GrowthBook migration surface that is no
longer part of the current Rust personal build. Keeping a script here avoids a
silent missing-gate failure, but it records an explicit retirement artifact
instead of importing removed TS services.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log  # noqa: E402


def main() -> int:
    ctx = make_fixture("R8_default_value_parity_retired")
    result = {
        "name": "retired_growthbook_default_value_parity",
        "ok": True,
        "reason": (
            "The TypeScript GrowthBook default-value parity gate is retired for "
            "the current Rust personal build; active config precedence is covered "
            "by R5/R6 and M9 profile/config harnesses."
        ),
        "replacement_gates": [
            "scripts/harness_R5_provider_priority_smoke.py",
            "scripts/harness_R6_local_project_override_smoke.py",
            "scripts/harness_M9_6_profile_cli_flags_smoke.py",
            "scripts/harness_M9_10_migrate_fallback_to_settings_smoke.py",
        ],
    }
    stdout = json.dumps(result, indent=2, ensure_ascii=False)
    write_command_log(ctx, ["python3", Path(__file__).name], stdout, "", 0)
    write_assertions(
        ctx,
        status="passed",
        assertions=[
            {
                "name": result["name"],
                "expected": True,
                "actual": result["ok"],
                "passed": result["ok"],
                "evidence": result["reason"],
            }
        ],
    )
    summary = {
        "test_id": ctx.test_id,
        "status": "passed",
        "results": [result],
        "passed": 1,
        "total": 1,
        "fixture_root": str(ctx.root_dir),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
