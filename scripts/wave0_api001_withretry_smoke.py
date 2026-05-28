#!/usr/bin/env python3
"""Wave 0 — NEEDS-DESIGN-API-001 focused smoke (static-only).

Verifies that services/api/withRetry.ts:354 has migrated from
`process.env.USER_TYPE === 'external'` to `getUserType() === 'external'`
and that the import for getUserType is present.

Why static-only (not runtime):
  * withRetry.ts transitively imports the entire mossen-sdk runtime
    (services/api/mossenSdk.js, analytics/index.js, fastMode.js, etc.).
    bun -e cannot resolve those deferred runtime modules against source.
  * The change is a single-token swap (`process.env.USER_TYPE` →
    `getUserType()`); a static structural assertion is sufficient.
  * Live 5x-529 trigger requires VCR fixture or real backend, both out of
    scope for Wave 0 hotfix smoke (Allen explicitly excluded these).
  * Recorded here per Allen's directive to not fake runtime passes.

No LLM, no real backend, no ~/.mossen write. Pure file read + regex.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WITHRETRY_PATH = ROOT / "services" / "api" / "withRetry.ts"


def static_assertion() -> dict[str, object]:
    text = WITHRETRY_PATH.read_text(encoding="utf-8")

    findings: dict[str, object] = {
        "import_getusertype_present": False,
        "old_pattern_present": False,
        "new_pattern_present": False,
        "is_sandbox_skip_present": False,
        "persistent_retry_skip_present": False,
        "repeated_529_throw_present": False,
    }

    # Import line: `import { getUserType } from '../../utils/userType.js'`
    if re.search(
        r"import\s*\{\s*getUserType\s*\}\s*from\s*'\.\./\.\./utils/userType\.js'",
        text,
    ):
        findings["import_getusertype_present"] = True

    # Old pattern (must NOT be present in the bailout block)
    bailout_block_re = re.compile(
        r"if\s*\(\s*[\s\S]{0,200}?process\.env\.USER_TYPE\s*===\s*'external'\s*&&\s*[\s\S]{0,300}?REPEATED_529_ERROR_MESSAGE",
        re.MULTILINE,
    )
    if bailout_block_re.search(text):
        findings["old_pattern_present"] = True

    # New pattern (must be present, replacing old)
    new_block_re = re.compile(
        r"if\s*\(\s*[\s\S]{0,200}?getUserType\(\)\s*===\s*'external'\s*&&\s*[\s\S]{0,300}?REPEATED_529_ERROR_MESSAGE",
        re.MULTILINE,
    )
    if new_block_re.search(text):
        findings["new_pattern_present"] = True

    # Skip conditions must remain in the same block
    if "!process.env.IS_SANDBOX" in text:
        findings["is_sandbox_skip_present"] = True
    if "!isPersistentRetryEnabled()" in text:
        findings["persistent_retry_skip_present"] = True

    # Throw target must be intact
    if "REPEATED_529_ERROR_MESSAGE" in text:
        findings["repeated_529_throw_present"] = True

    return findings


def main() -> int:
    failures: list[str] = []
    findings = static_assertion()

    if not findings["import_getusertype_present"]:
        failures.append(
            "import { getUserType } from '../../utils/userType.js' missing — "
            "API-001 not applied"
        )
    if findings["old_pattern_present"]:
        failures.append(
            "process.env.USER_TYPE === 'external' still present inside the bailout "
            "if-block — API-001 not applied"
        )
    if not findings["new_pattern_present"]:
        failures.append(
            "getUserType() === 'external' missing inside the bailout if-block — "
            "API-001 not applied"
        )
    if not findings["is_sandbox_skip_present"]:
        failures.append(
            "!process.env.IS_SANDBOX skip condition missing — API-001 must preserve"
        )
    if not findings["persistent_retry_skip_present"]:
        failures.append(
            "!isPersistentRetryEnabled() skip condition missing — API-001 must preserve"
        )
    if not findings["repeated_529_throw_present"]:
        failures.append(
            "REPEATED_529_ERROR_MESSAGE throw target missing — API-001 broke bailout"
        )

    report = {
        "name": "wave0_api001_withretry_smoke",
        "mode": "static-only",
        "mode_reason": (
            "withRetry.ts transitively imports mossenSdk + analytics + fastMode "
            "(deferred runtime modules unresolvable via bun -e against source). "
            "Live 5x-529 trigger needs VCR fixture / real backend, out of scope "
            "for Wave 0 hotfix smoke."
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
