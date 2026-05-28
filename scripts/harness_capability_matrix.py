#!/usr/bin/env python3
"""Generate the Harness Manifest / Capability Matrix v1 report.

This report is intentionally capability-first. A harness script can only count
as product evidence after it is mapped to a capability and has current
`assertions.json` evidence under /tmp/mossen-harness.
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import re
from collections import Counter
from datetime import datetime
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MATRIX = ROOT / "docs" / "capability_matrix.v1.json"
DEFAULT_HARNESS_ROOT = Path("/tmp/mossen-harness")
STATUS_ORDER = ("fail", "missing", "stale", "partial", "pass")
BAD_LAUNCHERS = ("run-mossen.sh", "run-bun-featured.sh")


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def discover_assertions(harness_root: Path) -> dict[str, dict[str, Any]]:
    assertions: dict[str, dict[str, Any]] = {}
    if not harness_root.exists():
        return assertions
    for path in sorted(harness_root.glob("*/artifacts/assertions.json")):
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as exc:
            test_id = path.parent.parent.name
            payload = {
                "test_id": test_id,
                "status": "load_error",
                "error": str(exc),
            }
        test_id = str(payload.get("test_id") or path.parent.parent.name)
        payload["_source_file"] = str(path)
        payload["_artifacts_dir"] = str(path.parent)
        assertions[test_id] = payload
    return assertions


def discover_mr_scripts() -> list[str]:
    scripts = []
    for path in sorted((ROOT / "scripts").glob("harness_[MR]*.py")):
        scripts.append(str(path.relative_to(ROOT)))
    return scripts


def expand_script_globs(patterns: list[str]) -> tuple[list[str], list[str]]:
    matched: list[str] = []
    missing_patterns: list[str] = []
    repo_files = [str(path.relative_to(ROOT)) for path in (ROOT / "scripts").glob("*.py")]
    for pattern in patterns:
        hits = sorted(path for path in repo_files if fnmatch.fnmatch(path, pattern))
        if not hits:
            missing_patterns.append(pattern)
            continue
        matched.extend(hits)
    return sorted(set(matched)), missing_patterns


def nominal_test_prefix(script: str) -> str | None:
    name = Path(script).name
    match = re.match(r"harness_M(\d+)(?:_(\d+))?", name)
    if match:
        module = match.group(1)
        case = match.group(2)
        return f"M{module}.{case}" if case else f"M{module}"
    match = re.match(r"harness_R(\d+)", name)
    if match:
        return f"R{match.group(1)}"
    match = re.match(r"wave_w(\d+)", name)
    if match:
        return f"W{match.group(1)}"
    return None


def test_id_matches_prefix(test_id: str, prefix: str) -> bool:
    return (
        test_id == prefix
        or test_id.startswith(prefix + "_")
        or test_id.startswith(prefix + ".")
        or test_id.startswith(prefix + "-")
    )


def script_launcher_issues(script: str) -> list[str]:
    path = ROOT / script
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        return [f"unreadable: {exc}"]
    return [launcher for launcher in BAD_LAUNCHERS if launcher in text]


def reproduce_command(script: str) -> str:
    return f"python3 {script}"


def status_from_assertions(assertions: list[dict[str, Any]]) -> str:
    statuses = {str(item.get("status", "load_error")) for item in assertions}
    if statuses & {"failed", "load_error"}:
        return "fail"
    if statuses & {"blocked", "skipped"}:
        return "partial"
    if statuses and statuses <= {"passed"}:
        return "pass"
    return "missing"


def evaluate_capability(
    capability: dict[str, Any],
    assertions: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    verification = capability.get("verification", {})
    script_globs = list(verification.get("script_globs", []))
    scripts, missing_patterns = expand_script_globs(script_globs)

    script_rows = []
    evidence_statuses = []
    stale_scripts = []
    launcher_drift = []
    assertion_ids = set()

    for script in scripts:
        prefix = nominal_test_prefix(script)
        matched = []
        if prefix:
            matched = [
                payload
                for test_id, payload in sorted(assertions.items())
                if test_id_matches_prefix(test_id, prefix)
            ]
        script_status = status_from_assertions(matched) if matched else "stale"
        issues = script_launcher_issues(script)
        if issues:
            script_status = "stale"
            launcher_drift.append({"script": script, "retired_launchers": issues})
        if script_status == "stale":
            stale_scripts.append(script)
        evidence_statuses.append(script_status)
        for payload in matched:
            assertion_ids.add(str(payload.get("test_id")))
        script_rows.append(
            {
                "script": script,
                "reproduce_command": reproduce_command(script),
                "nominal_test_prefix": prefix,
                "status": script_status,
                "assertion_ids": [str(item.get("test_id")) for item in matched],
                "assertion_files": [
                    str(item.get("_source_file")) for item in matched if item.get("_source_file")
                ],
                "artifact_dirs": [
                    str(item.get("_artifacts_dir")) for item in matched if item.get("_artifacts_dir")
                ],
                "launcher_issues": issues,
            }
        )

    if missing_patterns or not scripts:
        status = "missing"
    elif any(item == "fail" for item in evidence_statuses):
        status = "fail"
    elif any(item == "stale" for item in evidence_statuses):
        status = "stale"
    elif any(item == "partial" for item in evidence_statuses):
        status = "partial"
    elif evidence_statuses and all(item == "pass" for item in evidence_statuses):
        status = "pass"
    else:
        status = "missing"

    matched_assertions = [assertions[test_id] for test_id in sorted(assertion_ids)]
    failed_invariants = []
    if status != "pass":
        if missing_patterns:
            failed_invariants.append("declared script globs must resolve to real scripts")
        if stale_scripts:
            failed_invariants.append("every mapped script must have current assertions.json evidence")
        if launcher_drift:
            failed_invariants.append("retired launchers cannot count as current product evidence")
        if any(str(item.get("status")) in {"failed", "load_error"} for item in matched_assertions):
            failed_invariants.append("matched current assertions must pass")
        if any(str(item.get("status")) in {"blocked", "skipped"} for item in matched_assertions):
            failed_invariants.append("matched current assertions must not be blocked or skipped")

    return {
        "id": capability["id"],
        "title": capability.get("title", capability["id"]),
        "status": status,
        "user_entries": capability.get("user_entries", []),
        "launcher": capability.get("launcher"),
        "code_paths": capability.get("code_paths", []),
        "invariants": capability.get("invariants", []),
        "failed_invariants": failed_invariants,
        "script_globs": script_globs,
        "scripts": script_rows,
        "missing_script_globs": missing_patterns,
        "assertion_ids": sorted(assertion_ids),
        "reproduce_commands": [row["reproduce_command"] for row in script_rows],
        "evidence_paths": sorted(
            {
                path
                for row in script_rows
                for path in [*row.get("assertion_files", []), *row.get("artifact_dirs", [])]
            }
        ),
        "artifacts": sorted(
            {
                str(item.get("_artifacts_dir"))
                for item in matched_assertions
                if item.get("_artifacts_dir")
            }
        ),
        "launcher_drift": launcher_drift,
    }


def build_script_mapping(capabilities: list[dict[str, Any]]) -> dict[str, list[str]]:
    mapping: dict[str, list[str]] = {}
    for capability in capabilities:
        scripts, _ = expand_script_globs(
            list(capability.get("verification", {}).get("script_globs", []))
        )
        for script in scripts:
            mapping.setdefault(script, []).append(str(capability["id"]))
    return mapping


def build_report(matrix_path: Path, harness_root: Path) -> dict[str, Any]:
    matrix = load_json(matrix_path)
    assertions = discover_assertions(harness_root)
    capabilities = matrix.get("capabilities", [])
    evaluated = [evaluate_capability(cap, assertions) for cap in capabilities]

    mapping = build_script_mapping(capabilities)
    mr_scripts = discover_mr_scripts()
    unmapped_scripts = [script for script in mr_scripts if script not in mapping]
    multi_mapped_scripts = {
        script: caps for script, caps in sorted(mapping.items()) if len(caps) > 1
    }

    status_counts = Counter(item["status"] for item in evaluated)
    return {
        "schema_version": 1,
        "generated_at": datetime.now().isoformat(),
        "matrix": str(matrix_path.relative_to(ROOT)),
        "harness_root": str(harness_root),
        "summary_by_status": {status: status_counts.get(status, 0) for status in STATUS_ORDER},
        "total_capabilities": len(evaluated),
        "capabilities": evaluated,
        "script_coverage": {
            "total_mr_scripts": len(mr_scripts),
            "mapped_mr_scripts": len([script for script in mr_scripts if script in mapping]),
            "unmapped_mr_scripts": unmapped_scripts,
            "multi_mapped_mr_scripts": multi_mapped_scripts,
        },
        "assertion_evidence": {
            "total_assertion_files": len(assertions),
            "test_ids": sorted(assertions),
        },
    }


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# Harness Capability Matrix Report",
        "",
        f"> generated_at: {report['generated_at']}",
        f"> matrix: `{report['matrix']}`",
        f"> harness_root: `{report['harness_root']}`",
        "",
        "## Summary",
        "",
        "| status | count |",
        "|---|---:|",
    ]
    for status in STATUS_ORDER:
        lines.append(f"| `{status}` | {report['summary_by_status'].get(status, 0)} |")
    coverage = report["script_coverage"]
    lines.extend(
        [
            "",
            "## Script Coverage",
            "",
            f"- M/R scripts: {coverage['mapped_mr_scripts']}/{coverage['total_mr_scripts']} mapped",
            f"- unmapped: {len(coverage['unmapped_mr_scripts'])}",
            f"- multi_mapped: {len(coverage['multi_mapped_mr_scripts'])}",
            "",
            "## Capabilities",
            "",
            "| capability | status | scripts | current evidence | failed invariant |",
            "|---|---|---:|---:|---|",
        ]
    )
    for cap in report["capabilities"]:
        failed = "; ".join(cap["failed_invariants"][:2])
        lines.append(
            "| `{}` | `{}` | {} | {} | {} |".format(
                cap["id"],
                cap["status"],
                len(cap["scripts"]),
                len(cap["assertion_ids"]),
                failed.replace("|", "\\|"),
            )
        )

    attention = [
        cap
        for cap in report["capabilities"]
        if cap["status"] != "pass" or cap["launcher_drift"] or cap["missing_script_globs"]
    ]
    if attention:
        lines.extend(["", "## Needs Attention", ""])
        for cap in attention:
            lines.append(f"### {cap['id']} ({cap['status']})")
            if cap["code_paths"]:
                lines.append("- code_paths:")
                for path in cap["code_paths"][:8]:
                    lines.append(f"  - `{path}`")
            if cap["failed_invariants"]:
                for item in cap["failed_invariants"]:
                    lines.append(f"- invariant: {item}")
            stale = [row for row in cap["scripts"] if row["status"] == "stale"]
            for row in stale[:8]:
                lines.append(
                    f"- stale: `{row['script']}` expected `{row['nominal_test_prefix']}`"
                )
                lines.append(f"  - reproduce: `{row['reproduce_command']}`")
                if row["assertion_files"]:
                    lines.append(f"  - evidence: `{row['assertion_files'][0]}`")
                else:
                    lines.append("  - evidence: missing `artifacts/assertions.json`")
            for item in cap["missing_script_globs"]:
                lines.append(f"- missing_glob: `{item}`")
            for item in cap["launcher_drift"]:
                lines.append(
                    f"- launcher_drift: `{item['script']}` uses {item['retired_launchers']}"
                )
            lines.append("")

    if coverage["unmapped_mr_scripts"]:
        lines.extend(["", "## Unmapped M/R Scripts", ""])
        for script in coverage["unmapped_mr_scripts"]:
            lines.append(f"- `{script}`")

    lines.extend(["", "## Evidence Index", ""])
    for cap in report["capabilities"]:
        lines.append(f"### {cap['id']} ({cap['status']})")
        lines.append(f"- launcher: `{cap.get('launcher') or ''}`")
        if cap["code_paths"]:
            lines.append("- code_paths:")
            for path in cap["code_paths"][:8]:
                lines.append(f"  - `{path}`")
        commands = cap.get("reproduce_commands", [])
        if commands:
            lines.append("- reproduce:")
            for command in commands[:8]:
                lines.append(f"  - `{command}`")
            if len(commands) > 8:
                lines.append(f"  - ... {len(commands) - 8} more")
        evidence = cap.get("evidence_paths", [])
        if evidence:
            lines.append("- evidence:")
            for path in evidence[:8]:
                lines.append(f"  - `{path}`")
            if len(evidence) > 8:
                lines.append(f"  - ... {len(evidence) - 8} more")
        else:
            lines.append("- evidence: missing")
        lines.append("")

    lines.extend(
        [
            "",
            "## Rule",
            "",
            "A script pass only counts after it maps to a capability and has current assertion evidence.",
        ]
    )
    return "\n".join(lines) + "\n"


def has_matrix_failures(report: dict[str, Any]) -> bool:
    bad_statuses = {status for status in STATUS_ORDER if status != "pass"}
    return (
        any(report["summary_by_status"].get(status, 0) for status in bad_statuses)
        or bool(report["script_coverage"]["unmapped_mr_scripts"])
        or bool(report["script_coverage"]["multi_mapped_mr_scripts"])
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--matrix", type=Path, default=DEFAULT_MATRIX)
    parser.add_argument("--harness-root", type=Path, default=DEFAULT_HARNESS_ROOT)
    parser.add_argument("--output-dir", type=Path, default=ROOT)
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--quiet", action="store_true")
    args = parser.parse_args()

    matrix_path = args.matrix if args.matrix.is_absolute() else ROOT / args.matrix
    output_dir = args.output_dir if args.output_dir.is_absolute() else ROOT / args.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    report = build_report(matrix_path, args.harness_root)
    json_path = output_dir / "harness-capability-report.json"
    md_path = output_dir / "harness-capability-report.md"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    md_path.write_text(render_markdown(report), encoding="utf-8")

    if not args.quiet:
        print(
            json.dumps(
                {
                    "json_report": str(json_path),
                    "md_report": str(md_path),
                    "summary_by_status": report["summary_by_status"],
                    "script_coverage": report["script_coverage"],
                },
                indent=2,
                ensure_ascii=False,
            )
        )

    if args.strict and has_matrix_failures(report):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
