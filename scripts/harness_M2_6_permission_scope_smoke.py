#!/usr/bin/env python3
"""
M2.6 - current Rust permission source smoke.

The current permission loader keeps rules tagged by source and the dialogue
permission layer applies deny-before-allow semantics across those sources. This
gate validates user/project/local/policy loading and the source-aware rule path.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    steps = [
        Step(
            name="loader_preserves_user_project_local_sources",
            command=cargo_test(
                "-p",
                "mossen-utils",
                "load_all_permission_rules_preserves_user_project_local_sources",
            ),
            timeout_secs=180,
        ),
        Step(
            name="managed_policy_only_ignores_editable_sources",
            command=cargo_test(
                "-p",
                "mossen-utils",
                "managed_only_permission_rules_ignore_editable_sources",
            ),
            timeout_secs=180,
        ),
        Step(
            name="dialogue_deny_precedes_allow_across_sources",
            command=cargo_test(
                "-p",
                "mossen-agent",
                "session_permission_rules_deny_precedes_allow",
            ),
            timeout_secs=180,
        ),
    ]
    checks = [
        source_check(
            "permission_loader_maps_all_supported_sources",
            "crates/mossen-utils/src/permissions/permissions_loader.rs",
            [
                "\"userSettings\" => PermissionRuleSource::UserSettings",
                "\"projectSettings\" => PermissionRuleSource::ProjectSettings",
                "\"localSettings\" => PermissionRuleSource::LocalSettings",
                "\"policySettings\" => PermissionRuleSource::PolicySettings",
                "allowManagedPermissionRulesOnly",
            ],
        ),
        source_check(
            "permission_rules_keep_source_priority_inventory",
            "crates/mossen-utils/src/permissions/permissions.rs",
            [
                "pub const PERMISSION_RULE_SOURCES",
                "PermissionRuleSource::UserSettings",
                "PermissionRuleSource::ProjectSettings",
                "PermissionRuleSource::LocalSettings",
                "PermissionRuleSource::PolicySettings",
            ],
        ),
    ]
    return run_context_harness(
        test_id="M2.6_permission_scope_current_rust",
        script_name=Path(__file__).name,
        steps=steps,
        checks=checks,
        design_note=(
            "M2.6 validates current Rust permission rule source loading and "
            "deny-before-allow application across sources."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
