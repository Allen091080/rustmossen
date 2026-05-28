#!/usr/bin/env python3
"""M17.3 - TUI copy command contract evidence."""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions


CHECKS = [
    (
        "tui_copy_supports_transcript_alias",
        "crates/mossen-tui/src/app.rs",
        'matches!(trimmed, "transcript" | "all")',
    ),
    (
        "tui_copy_uses_export_transcript_text",
        "crates/mossen-tui/src/app.rs",
        "self.export_transcript_text()",
    ),
    (
        "tui_copy_has_pure_payload_regression_test",
        "crates/mossen-tui/src/app.rs",
        "copy_command_payload_supports_latest_response_and_full_transcript",
    ),
    (
        "non_tui_copy_fails_closed_for_transcript",
        "crates/mossen-commands/src/copy.rs",
        "Cannot copy the transcript from this command runner",
    ),
    (
        "slash_help_mentions_transcript_copy",
        "crates/mossen-commands/src/copy.rs",
        "Usage: /copy [N|transcript|all]",
    ),
]


def main() -> int:
    ctx = make_fixture("M17.3_tui_copy_contract")
    source_cache: dict[str, str] = {}
    assertions = []
    for name, relative_path, token in CHECKS:
        path = ROOT / relative_path
        text = source_cache.get(relative_path)
        if text is None:
            text = path.read_text(encoding="utf-8")
            source_cache[relative_path] = text
        passed = token in text
        assertions.append(
            {
                "name": name,
                "expected": token,
                "actual": token if passed else "<missing>",
                "passed": passed,
                "evidence": relative_path,
            }
        )

    ok = all(item["passed"] for item in assertions)
    checks_path = ctx.artifacts_dir / "copy_contract_checks.json"
    checks_path.write_text(json.dumps(assertions, indent=2), encoding="utf-8")
    write_assertions(
        ctx,
        status="passed" if ok else "failed",
        assertions=assertions,
        extra_artifacts={"copy_contract_checks": str(checks_path)},
    )

    summary = {
        "test_id": ctx.test_id,
        "status": "passed" if ok else "failed",
        "passed": sum(1 for item in assertions if item["passed"]),
        "total": len(assertions),
        "fixture_root": str(ctx.root_dir),
    }
    print(json.dumps(summary, indent=2))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
