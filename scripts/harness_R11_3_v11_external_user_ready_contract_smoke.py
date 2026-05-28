#!/usr/bin/env python3
"""R11.3 - V1.1 External User Ready contract smoke.

This smoke keeps the user-facing V1.1 acceptance criteria wired to concrete
docs, workflows, and focused harnesses. It is not a substitute for the
underlying runtime tests; it fails when those tests stop being part of the
release path.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions


def read(relative_path: str) -> str:
    return (ROOT / relative_path).read_text(encoding="utf-8")


def contains_all(relative_path: str, needles: list[str]) -> tuple[bool, list[str]]:
    text = read(relative_path)
    missing = [needle for needle in needles if needle not in text]
    return not missing, missing


def assertion(name: str, ok: bool, evidence: str, missing: list[str] | None = None) -> dict[str, Any]:
    return {
        "name": name,
        "expected": "all required evidence present",
        "actual": "present" if ok else f"missing: {missing}",
        "passed": ok,
        "evidence": evidence,
    }


def main() -> int:
    ctx = make_fixture("R11.3_v11_external_user_ready_contract")
    assertions: list[dict[str, Any]] = []

    checks = [
        (
            "readme_five_minute_quick_start_is_actionable",
            "README.md",
            [
                "## 5-Minute Quick Start",
                "cargo install --path crates/mossen-cli --bin mossen --locked",
                "mossen --add-model-profile my-model",
                "mossen --set-model-profile my-model",
                "mossen --test-model-profile my-model --timeout 30000",
                "mossen --cwd /path/to/project",
                "/doctor",
                "scripts/start-mossen.sh",
            ],
        ),
        (
            "zh_readme_matches_external_user_setup_path",
            "README.zh-CN.md",
            [
                "5 分钟快速启动",
                "cargo install --path crates/mossen-cli --bin mossen --locked",
                "mossen --add-model-profile my-model",
                "mossen --set-model-profile my-model",
                "mossen --test-model-profile my-model --timeout 30000",
                "/doctor",
                "scripts/start-mossen.sh",
            ],
        ),
        (
            "doctor_covers_common_model_config_failures_without_secret_output",
            "crates/mossen-agent/src/services/config/doctor.rs",
            [
                "profiles_not_object",
                "no_valid_settings_profiles",
                "some_settings_profiles_invalid",
                "active_profile_not_found",
                "no_model_profile",
                "custom_backend_env_incomplete",
                "base_urls_redacted: true",
                "api_keys_redacted: true",
                "next_commands",
            ],
        ),
        (
            "structured_doctor_has_no_model_and_redaction_tests",
            "crates/mossen-cli/src/structured_io.rs",
            [
                "slash_command_doctor_returns_redacted_runtime_health_snapshot",
                "slash_command_doctor_guides_when_model_profile_is_missing",
                "slash_command_doctor_warns_when_active_profile_is_missing",
                "slash_command_doctor_reports_invalid_model_profiles",
                "slash_command_doctor_reports_partial_custom_backend_env",
                "slash_command_doctor_redacts_configured_model_profile_secrets",
                "slash_command_doctor_reports_memory_and_compact_checks",
            ],
        ),
        (
            "default_ci_runs_on_push_and_pull_request",
            ".github/workflows/ci.yml",
            [
                "push:",
                "pull_request:",
                "cargo fmt --all -- --check",
                "cargo check --workspace",
                "python3 scripts/harness_R11_3_v11_external_user_ready_contract_smoke.py",
            ],
        ),
        (
            "full_tests_workflow_is_manual_and_covers_v11_gates",
            ".github/workflows/tests.yml",
            [
                "workflow_dispatch:",
                "cargo test --workspace --lib --bins",
                "python3 scripts/harness_M8_1_command_inventory_real_smoke.py",
                "python3 scripts/harness_M8_4_hidden_commands_smoke.py",
                "python3 scripts/harness_M9_2_auth_missing_clear_error_smoke.py",
                "python3 scripts/harness_M9_14_doctor_common_config_smoke.py",
                "python3 scripts/harness_M9_13_provider_mock_protocol_matrix.py",
                "python3 scripts/harness_M10_4_async_agent_taskoutput_smoke.py",
                "python3 scripts/harness_M10_6_parallel_agents_taskoutput_smoke.py",
                "python3 scripts/harness_M17_1_tui_rendering_interaction_smoke.py",
                "python3 scripts/harness_M17_2_tui_agent_input_responsiveness_smoke.py",
                "python3 scripts/harness_M17_3_tui_copy_contract_smoke.py",
                "python3 scripts/harness_R11_2_package_install_smoke.py",
                "python3 scripts/harness_R11_3_v11_external_user_ready_contract_smoke.py",
                "python3 scripts/v11_external_user_ready_status.py",
            ],
        ),
        (
            "capability_matrix_tracks_v11_external_user_surfaces",
            "docs/capability_matrix.v1.json",
            [
                '"id": "slash_commands"',
                '"/help lists only visible enabled commands"',
                '"id": "model_provider_profiles"',
                '"all supported provider protocols have local mock coverage"',
                '"id": "subagent_lifecycle"',
                '"parallel Agents expose unique visible task ids and complete feedback"',
                '"id": "tui_rendering_interaction"',
                '"typing remains responsive during background agent activity"',
                '"id": "release_readiness_and_install"',
            ],
        ),
    ]

    for name, relative_path, needles in checks:
        ok, missing = contains_all(relative_path, needles)
        assertions.append(assertion(name, ok, relative_path, missing))

    artifacts_path = ctx.artifacts_dir / "v11_contract_checks.json"
    artifacts_path.write_text(json.dumps(assertions, indent=2, ensure_ascii=False), encoding="utf-8")
    ok = all(item["passed"] for item in assertions)
    write_assertions(
        ctx,
        status="passed" if ok else "failed",
        assertions=assertions,
        extra_artifacts={"v11_contract_checks": str(artifacts_path)},
    )
    print(
        json.dumps(
            {
                "test_id": ctx.test_id,
                "status": "passed" if ok else "failed",
                "passed": sum(1 for item in assertions if item["passed"]),
                "total": len(assertions),
                "fixture_root": str(ctx.root_dir),
            },
            indent=2,
            ensure_ascii=False,
        )
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
