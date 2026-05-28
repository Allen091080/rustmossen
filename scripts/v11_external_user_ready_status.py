#!/usr/bin/env python3
"""Report V1.1 External User Ready status from current evidence."""

from __future__ import annotations

import argparse
import json
from datetime import datetime
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_HARNESS_ROOT = Path("/tmp/mossen-harness")
DEFAULT_OUTPUT_DIR = Path("/tmp/mossen-v11-status")


def read_text(relative_path: str) -> str:
    return (ROOT / relative_path).read_text(encoding="utf-8")


def discover_assertions(harness_root: Path) -> dict[str, dict[str, Any]]:
    assertions: dict[str, dict[str, Any]] = {}
    if not harness_root.exists():
        return assertions
    for path in sorted(harness_root.glob("*/artifacts/assertions.json")):
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as exc:
            payload = {
                "test_id": path.parent.parent.name,
                "status": "load_error",
                "error": str(exc),
            }
        test_id = str(payload.get("test_id") or path.parent.parent.name)
        payload["_source_file"] = str(path)
        assertions[test_id] = payload
    return assertions


def passed_assertion(assertions: dict[str, dict[str, Any]], test_id: str) -> tuple[bool, str]:
    payload = assertions.get(test_id)
    if payload is None:
        return False, f"missing assertions for {test_id}"
    status = payload.get("status")
    source = payload.get("_source_file", "<unknown>")
    if status != "passed":
        return False, f"{test_id} status={status} source={source}"
    return True, source


def contains_all(relative_path: str, needles: list[str]) -> tuple[bool, str]:
    text = read_text(relative_path)
    missing = [needle for needle in needles if needle not in text]
    if missing:
        return False, f"{relative_path} missing {missing}"
    return True, relative_path


def check_group(checks: list[tuple[bool, str]]) -> tuple[str, list[str], list[str]]:
    passed = [detail for ok, detail in checks if ok]
    failed = [detail for ok, detail in checks if not ok]
    return ("passed" if not failed else "failed", passed, failed)


def criterion(
    criterion_id: int,
    title: str,
    checks: list[tuple[bool, str]],
) -> dict[str, Any]:
    status, passed, failed = check_group(checks)
    return {
        "id": criterion_id,
        "title": title,
        "status": status,
        "passed_evidence": passed,
        "failed_evidence": failed,
    }


def build_status(harness_root: Path) -> dict[str, Any]:
    assertions = discover_assertions(harness_root)
    ci = " .github/workflows/ci.yml"
    tests = ".github/workflows/tests.yml"

    criteria = [
        criterion(
            1,
            "New users can get running from the README in about five minutes",
            [
                contains_all(
                    "README.md",
                    [
                        "## 5-Minute Quick Start",
                        "cargo install --path crates/mossen-cli --bin mossen --locked",
                        "mossen --add-model-profile my-model",
                        "mossen --test-model-profile my-model --timeout 30000",
                        "mossen --cwd /path/to/project",
                    ],
                ),
                contains_all(
                    "README.zh-CN.md",
                    [
                        "5 分钟快速启动",
                        "cargo install --path crates/mossen-cli --bin mossen --locked",
                        "mossen --add-model-profile my-model",
                        "mossen --test-model-profile my-model --timeout 30000",
                    ],
                ),
                passed_assertion(assertions, "R11.2_package_install_smoke"),
            ],
        ),
        criterion(
            2,
            "Missing model configuration has clear guidance",
            [
                passed_assertion(assertions, "M9.2_auth_missing_clear_error_current_rust"),
                passed_assertion(assertions, "M9.14_doctor_common_config_diagnostics"),
                contains_all("README.md", ["/doctor", "model is not configured"]),
            ],
        ),
        criterion(
            3,
            "/doctor identifies common configuration problems",
            [
                passed_assertion(assertions, "M9.14_doctor_common_config_diagnostics"),
                contains_all(
                    "crates/mossen-agent/src/services/config/doctor.rs",
                    [
                        "profiles_not_object",
                        "no_valid_settings_profiles",
                        "active_profile_not_found",
                        "no_model_profile",
                        "custom_backend_env_incomplete",
                    ],
                ),
            ],
        ),
        criterion(
            4,
            "openai-compatible, openai-responses, and anthropic mock tests are stable",
            [
                passed_assertion(assertions, "M9.13_provider_mock_protocol_matrix"),
                contains_all(
                    "scripts/harness_M9_13_provider_mock_protocol_matrix.py",
                    [
                        "openai-compatible",
                        "openai-responses",
                        "anthropic",
                        "harness_executes_tool_loop_through_openai_responses_protocol",
                        "harness_executes_tool_loop_through_anthropic_protocol",
                    ],
                ),
            ],
        ),
        criterion(
            5,
            "Sub-agent lifecycle has complete feedback",
            [
                passed_assertion(assertions, "M10.4"),
                passed_assertion(assertions, "M10.6"),
                contains_all(
                    "docs/capability_matrix.v1.json",
                    [
                        "background Agent returns a retrievable task id",
                        "parallel Agents expose unique visible task ids and complete feedback",
                    ],
                ),
            ],
        ),
        criterion(
            6,
            "Default CI is greenable and the full test workflow is manually runnable",
            [
                contains_all(
                    " .github/workflows/ci.yml".strip(),
                    [
                        "push:",
                        "pull_request:",
                        "cargo fmt --all -- --check",
                        "cargo check --workspace",
                        "python3 scripts/harness_R11_3_v11_external_user_ready_contract_smoke.py",
                    ],
                ),
                contains_all(
                    tests,
                    [
                        "workflow_dispatch:",
                        "cargo test --workspace --lib --bins",
                        "python3 scripts/harness_R11_2_package_install_smoke.py",
                        "python3 scripts/harness_M9_14_doctor_common_config_smoke.py",
                        "python3 scripts/v11_external_user_ready_status.py",
                    ],
                ),
                passed_assertion(assertions, "R11.3_v11_external_user_ready_contract"),
            ],
        ),
        criterion(
            7,
            "/help does not show disconnected capabilities",
            [
                passed_assertion(assertions, "M8.1_command_inventory_current_rust"),
                passed_assertion(assertions, "M8.4_hidden_commands_current_rust"),
                contains_all(
                    "docs/capability_matrix.v1.json",
                    ["/help lists only visible enabled commands"],
                ),
            ],
        ),
        criterion(
            8,
            "TUI scroll, copy, and input latency are in focused repair coverage",
            [
                passed_assertion(assertions, "M17.1"),
                passed_assertion(assertions, "M17.2_tui_agent_input_responsiveness"),
                passed_assertion(assertions, "M17.3_tui_copy_contract"),
                contains_all(
                    "docs/capability_matrix.v1.json",
                    [
                        "scrolling can return to live tail",
                        "copy can export the current transcript without relying on mouse selection",
                        "typing remains responsive during background agent activity",
                    ],
                ),
            ],
        ),
    ]
    passed_count = sum(1 for item in criteria if item["status"] == "passed")
    return {
        "schema_version": 1,
        "generated_at": datetime.now().isoformat(),
        "goal": "V1.1 - External User Ready",
        "harness_root": str(harness_root),
        "ready": passed_count == len(criteria),
        "passed": passed_count,
        "total": len(criteria),
        "criteria": criteria,
    }


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# V1.1 External User Ready Status",
        "",
        f"- Generated: `{report['generated_at']}`",
        f"- Status: `{'ready' if report['ready'] else 'not-ready'}`",
        f"- Passed: `{report['passed']}/{report['total']}`",
        "",
        "| # | Criterion | Status |",
        "|---|---|---|",
    ]
    for item in report["criteria"]:
        lines.append(f"| {item['id']} | {item['title']} | `{item['status']}` |")
    lines.append("")
    failed = [item for item in report["criteria"] if item["status"] != "passed"]
    if failed:
        lines.append("## Failed Evidence")
        for item in failed:
            lines.append(f"- {item['id']}. {item['title']}")
            for detail in item["failed_evidence"]:
                lines.append(f"  - {detail}")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--harness-root", type=Path, default=DEFAULT_HARNESS_ROOT)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR)
    args = parser.parse_args()

    report = build_status(args.harness_root)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    json_path = args.output_dir / "v11-external-user-ready-status.json"
    md_path = args.output_dir / "v11-external-user-ready-status.md"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    md_path.write_text(render_markdown(report), encoding="utf-8")
    print(
        json.dumps(
            {
                "ready": report["ready"],
                "passed": report["passed"],
                "total": report["total"],
                "json": str(json_path),
                "markdown": str(md_path),
            },
            indent=2,
            ensure_ascii=False,
        )
    )
    return 0 if report["ready"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
