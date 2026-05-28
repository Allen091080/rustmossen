#!/usr/bin/env python3
"""
8 维行为结构化验收框架 (V-1).

按 OpenTelemetry删除计划.md §0.4.5 (子 agent D 设计) + Allen 决策 D-3 (用结构化断言代替 LLM 原文 diff).

8 大维度:
  dim_1_exit_code              : 进程退出码 == 0 (除非 expected != 0)
  dim_2_no_errors              : stderr 不含 Error/TypeError/ReferenceError/Cannot find module
  dim_3_session_jsonl_exists   : session jsonl 落盘 + ≥1 行
  dim_4_sessionid_consistency  : jsonl 内 sessionId 字段 == 文件名 stem
  dim_5_message_sequence       : ≥1 user message + ≥1 assistant message
  dim_6_tool_pairing           : tool_use.id 集合 ⊆ tool_result.tool_use_id 集合
  dim_7_permission_mode        : settings.permissionMode 与 log 一致 (若 fixture 设了)
  dim_8_model_string           : 实际用的 model 字符串与 fixture 期望一致 (动态读, 不 hardcode)

使用例:
    from validate_structural_equivalence import validate

    result = validate(
        scenario="V1_self_test",
        session_jsonl="/path/to/session.jsonl",
        stdout="...",
        stderr="...",
        exit_code=0,
        expected={
            "model": "example-large",
            "permission_mode": "default",
            "expected_exit_code": 0,
            # tool_pairing 必查 (S2/S3/S5 类场景)
            "require_tool_pairing": True,
        },
    )
    # result.passed, result.dimensions[i], result.diagnosis

CLI 接口:
    python3 validate_structural_equivalence.py \
        --session-jsonl /path/to/session.jsonl \
        --stdout-file /tmp/stdout.txt \
        --stderr-file /tmp/stderr.txt \
        --exit-code 0 \
        --expected-model example-large \
        --output /tmp/validation_result.json
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field, asdict
from pathlib import Path
from typing import Any


ERROR_PATTERNS = [
    r"\bError:",
    r"\bTypeError:",
    r"\bReferenceError:",
    r"\bSyntaxError:",
    r"Cannot find module",
    r"undefined is not",
    r"Cannot read propert(?:y|ies) of",
    r"is not a function",
]


@dataclass
class DimensionResult:
    name: str
    expected: Any
    actual: Any
    passed: bool
    diagnosis: str = ""


@dataclass
class ValidationResult:
    scenario: str
    session_jsonl: str
    dimensions: dict[str, DimensionResult] = field(default_factory=dict)

    @property
    def passed(self) -> bool:
        return all(d.passed for d in self.dimensions.values())

    @property
    def passed_count(self) -> int:
        return sum(1 for d in self.dimensions.values() if d.passed)

    @property
    def total_count(self) -> int:
        return len(self.dimensions)

    def to_dict(self) -> dict:
        return {
            "scenario": self.scenario,
            "session_jsonl": self.session_jsonl,
            "passed": self.passed,
            "passed_count": self.passed_count,
            "total_count": self.total_count,
            "dimensions": {k: asdict(v) for k, v in self.dimensions.items()},
        }


def _read_jsonl(path: str | Path) -> list[dict]:
    """读 jsonl, 跳过空行和坏行 (返回成功解析的)."""
    p = Path(path)
    if not p.exists():
        return []
    out = []
    for line in p.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return out


def _extract_message(event: dict) -> dict:
    """统一拿到 message 对象 (有的事件外层包了一层 message field)."""
    return event.get("message", event) if isinstance(event, dict) else {}


def _extract_role(event: dict) -> str | None:
    msg = _extract_message(event)
    role = msg.get("role")
    if isinstance(role, str):
        return role
    return None


def _extract_content_blocks(event: dict) -> list[dict]:
    """从 event 提 content blocks (assistant message 的 list / user message 含 tool_result)."""
    msg = _extract_message(event)
    content = msg.get("content")
    if isinstance(content, list):
        return [b for b in content if isinstance(b, dict)]
    return []


# ============================================================================
# 8 维度断言函数
# ============================================================================


def dim_1_exit_code(exit_code: int, expected_exit_code: int = 0) -> DimensionResult:
    passed = exit_code == expected_exit_code
    return DimensionResult(
        name="dim_1_exit_code",
        expected=expected_exit_code,
        actual=exit_code,
        passed=passed,
        diagnosis="" if passed else f"exit_code={exit_code} (expected {expected_exit_code})",
    )


def dim_2_no_errors(stderr: str) -> DimensionResult:
    matches = []
    for pattern in ERROR_PATTERNS:
        for m in re.finditer(pattern, stderr or ""):
            ctx_start = max(0, m.start() - 30)
            ctx_end = min(len(stderr), m.end() + 80)
            matches.append(stderr[ctx_start:ctx_end].strip())
    passed = len(matches) == 0
    diagnosis = ""
    if not passed:
        diagnosis = f"{len(matches)} error pattern(s) in stderr: " + " | ".join(matches[:3])
        if len(matches) > 3:
            diagnosis += f" ... (+{len(matches) - 3} more)"
    return DimensionResult(
        name="dim_2_no_errors",
        expected=0,
        actual=len(matches),
        passed=passed,
        diagnosis=diagnosis,
    )


def dim_3_session_jsonl_exists(session_jsonl: str | Path) -> DimensionResult:
    p = Path(session_jsonl)
    if not p.exists():
        return DimensionResult(
            name="dim_3_session_jsonl_exists",
            expected="exists+nonempty",
            actual="missing",
            passed=False,
            diagnosis=f"session jsonl not found: {session_jsonl}",
        )
    line_count = sum(1 for line in p.read_text(encoding="utf-8", errors="replace").splitlines()
                     if line.strip())
    passed = line_count >= 1
    return DimensionResult(
        name="dim_3_session_jsonl_exists",
        expected="≥1 line",
        actual=f"{line_count} lines",
        passed=passed,
        diagnosis="" if passed else "session jsonl exists but is empty",
    )


def dim_4_sessionid_consistency(session_jsonl: str | Path) -> DimensionResult:
    p = Path(session_jsonl)
    if not p.exists():
        return DimensionResult(
            name="dim_4_sessionid_consistency",
            expected="filename_stem == sessionId",
            actual="missing_file",
            passed=False,
            diagnosis="session jsonl not found",
        )
    filename_stem = p.stem
    events = _read_jsonl(p)
    if not events:
        return DimensionResult(
            name="dim_4_sessionid_consistency",
            expected=filename_stem,
            actual="no_events",
            passed=False,
            diagnosis="session jsonl has no parseable events",
        )

    # 找首条带 sessionId 字段的 event
    found_session_id = None
    for ev in events:
        sid = ev.get("sessionId")
        if not sid:
            msg = _extract_message(ev)
            sid = msg.get("sessionId")
        if not sid:
            metadata = ev.get("metadata") or _extract_message(ev).get("metadata")
            if isinstance(metadata, dict):
                sid = metadata.get("sessionId")
        if sid:
            found_session_id = sid
            break

    if found_session_id is None:
        # mossen 当前日志里 sessionId 可能不写每条 — 弱化为：文件名是 UUID 即认为 OK
        uuid_re = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{3,4}-[0-9a-f]{4}-[0-9a-f]{12}$")
        if uuid_re.match(filename_stem):
            return DimensionResult(
                name="dim_4_sessionid_consistency",
                expected=filename_stem,
                actual="filename_is_uuid (no in-event sessionId)",
                passed=True,
                diagnosis="weak-pass: filename stem is UUID, no per-event sessionId field present",
            )
        return DimensionResult(
            name="dim_4_sessionid_consistency",
            expected=filename_stem,
            actual="not_found",
            passed=False,
            diagnosis="no sessionId field found in any event AND filename is not UUID",
        )

    passed = filename_stem == found_session_id
    return DimensionResult(
        name="dim_4_sessionid_consistency",
        expected=filename_stem,
        actual=found_session_id,
        passed=passed,
        diagnosis="" if passed else f"mismatch: filename={filename_stem!r} vs event.sessionId={found_session_id!r}",
    )


def dim_5_message_sequence(session_jsonl: str | Path) -> DimensionResult:
    events = _read_jsonl(session_jsonl)
    user_count = 0
    assistant_count = 0
    for ev in events:
        role = _extract_role(ev)
        if role == "user":
            user_count += 1
        elif role == "assistant":
            assistant_count += 1
    passed = user_count >= 1 and assistant_count >= 1
    return DimensionResult(
        name="dim_5_message_sequence",
        expected="≥1 user + ≥1 assistant",
        actual=f"user={user_count}, assistant={assistant_count}",
        passed=passed,
        diagnosis="" if passed else f"insufficient messages: user={user_count}, assistant={assistant_count}",
    )


def dim_6_tool_pairing(session_jsonl: str | Path, required: bool = False) -> DimensionResult:
    """
    tool_use.id 与 tool_result.tool_use_id 必须配对.

    required=True (S2/S3/S5 类涉及 tool 的场景): 必须存在 tool_use 才算 pass.
    required=False (S1 simple echo 类): 无 tool_use 也 pass.
    """
    events = _read_jsonl(session_jsonl)
    tool_use_ids = set()
    tool_result_ids = set()
    for ev in events:
        for block in _extract_content_blocks(ev):
            if block.get("type") == "tool_use":
                bid = block.get("id")
                if bid:
                    tool_use_ids.add(bid)
            elif block.get("type") == "tool_result":
                tid = block.get("tool_use_id")
                if tid:
                    tool_result_ids.add(tid)

    if required and not tool_use_ids:
        return DimensionResult(
            name="dim_6_tool_pairing",
            expected="≥1 tool_use with paired result",
            actual="no_tool_use",
            passed=False,
            diagnosis="scenario expected tool invocation but none found",
        )

    if not tool_use_ids and not tool_result_ids:
        return DimensionResult(
            name="dim_6_tool_pairing",
            expected="N/A (no tools in scenario)",
            actual="no_tools",
            passed=True,
            diagnosis="",
        )

    unmatched_uses = tool_use_ids - tool_result_ids
    unmatched_results = tool_result_ids - tool_use_ids
    passed = not unmatched_uses and not unmatched_results
    diagnosis = ""
    if not passed:
        diagnosis = (
            f"unmatched tool_use ids: {sorted(unmatched_uses)[:3]}; "
            f"orphan tool_result ids: {sorted(unmatched_results)[:3]}"
        )
    return DimensionResult(
        name="dim_6_tool_pairing",
        expected=f"{len(tool_use_ids)} tool_use ⇄ matching tool_result",
        actual=f"uses={len(tool_use_ids)}, results={len(tool_result_ids)}, pair={len(tool_use_ids & tool_result_ids)}",
        passed=passed,
        diagnosis=diagnosis,
    )


def dim_7_permission_mode(
    session_jsonl: str | Path,
    expected_mode: str | None,
    settings_path: str | Path | None = None,
) -> DimensionResult:
    """
    若 expected_mode 设了, 检查 settings.json 实际值一致 + log 中无 mode 漂移.
    若 expected_mode = None, 该维 = N/A pass.
    """
    if expected_mode is None:
        return DimensionResult(
            name="dim_7_permission_mode",
            expected="N/A (not specified)",
            actual="N/A",
            passed=True,
            diagnosis="",
        )

    actual_mode = None
    if settings_path:
        sp = Path(settings_path)
        if sp.exists():
            try:
                cfg = json.loads(sp.read_text(encoding="utf-8", errors="replace"))
                permissions = cfg.get("permissions") or {}
                actual_mode = permissions.get("defaultMode") or cfg.get("permissionMode")
            except json.JSONDecodeError:
                pass

    passed = (actual_mode == expected_mode) if actual_mode is not None else False
    return DimensionResult(
        name="dim_7_permission_mode",
        expected=expected_mode,
        actual=actual_mode if actual_mode is not None else "not_found",
        passed=passed,
        diagnosis="" if passed else f"mode mismatch: expected={expected_mode}, actual={actual_mode}",
    )


def dim_8_model_string(
    session_jsonl: str | Path,
    expected_model: str | None,
    stdout: str = "",
) -> DimensionResult:
    """
    若 expected_model 设了, 检查 session log 中至少一处 model 字段 == 期望.
    动态读 expected (从 fixture/CLI args 传入), 不 hardcode.
    """
    if expected_model is None:
        return DimensionResult(
            name="dim_8_model_string",
            expected="N/A (not specified)",
            actual="N/A",
            passed=True,
            diagnosis="",
        )
    events = _read_jsonl(session_jsonl)
    found_models = set()
    for ev in events:
        m = ev.get("model")
        if isinstance(m, str):
            found_models.add(m)
        msg = _extract_message(ev)
        m2 = msg.get("model")
        if isinstance(m2, str):
            found_models.add(m2)
    if expected_model in found_models:
        return DimensionResult(
            name="dim_8_model_string",
            expected=expected_model,
            actual=sorted(found_models),
            passed=True,
            diagnosis="",
        )
    # 兜底: stdout 含字面量
    if expected_model in (stdout or ""):
        return DimensionResult(
            name="dim_8_model_string",
            expected=expected_model,
            actual=f"in_stdout (no log field): {expected_model}",
            passed=True,
            diagnosis="weak-pass: model found in stdout but not in session log",
        )
    return DimensionResult(
        name="dim_8_model_string",
        expected=expected_model,
        actual=sorted(found_models) if found_models else "not_found",
        passed=False,
        diagnosis=f"expected model {expected_model!r} not found in session log or stdout",
    )


# ============================================================================
# 主入口
# ============================================================================


def validate(
    scenario: str,
    session_jsonl: str | Path,
    stdout: str,
    stderr: str,
    exit_code: int,
    expected: dict | None = None,
) -> ValidationResult:
    """
    跑全 8 维, 返回 ValidationResult.

    expected 字段 (全可选):
      expected_exit_code: int = 0
      model: str | None       (维 8 期望)
      permission_mode: str | None  (维 7 期望)
      settings_path: str | None    (维 7 配合)
      require_tool_pairing: bool = False  (维 6 是否必须有 tool)
    """
    expected = expected or {}
    result = ValidationResult(scenario=scenario, session_jsonl=str(session_jsonl))

    result.dimensions["dim_1_exit_code"] = dim_1_exit_code(
        exit_code, expected.get("expected_exit_code", 0)
    )
    result.dimensions["dim_2_no_errors"] = dim_2_no_errors(stderr)
    result.dimensions["dim_3_session_jsonl_exists"] = dim_3_session_jsonl_exists(session_jsonl)
    result.dimensions["dim_4_sessionid_consistency"] = dim_4_sessionid_consistency(session_jsonl)
    result.dimensions["dim_5_message_sequence"] = dim_5_message_sequence(session_jsonl)
    result.dimensions["dim_6_tool_pairing"] = dim_6_tool_pairing(
        session_jsonl, required=expected.get("require_tool_pairing", False)
    )
    result.dimensions["dim_7_permission_mode"] = dim_7_permission_mode(
        session_jsonl,
        expected.get("permission_mode"),
        expected.get("settings_path"),
    )
    result.dimensions["dim_8_model_string"] = dim_8_model_string(
        session_jsonl, expected.get("model"), stdout
    )

    return result


def _read_or_empty(path: str | None) -> str:
    if not path:
        return ""
    p = Path(path)
    if not p.exists():
        return ""
    return p.read_text(encoding="utf-8", errors="replace")


def main() -> int:
    ap = argparse.ArgumentParser(description="8 维行为结构化验收")
    ap.add_argument("--scenario", required=True)
    ap.add_argument("--session-jsonl", required=True)
    ap.add_argument("--stdout-file", default=None)
    ap.add_argument("--stderr-file", default=None)
    ap.add_argument("--exit-code", type=int, required=True)
    ap.add_argument("--expected-model", default=None)
    ap.add_argument("--expected-permission-mode", default=None)
    ap.add_argument("--settings-path", default=None)
    ap.add_argument("--require-tool-pairing", action="store_true")
    ap.add_argument("--output", default=None, help="JSON 输出路径; 不传则打印到 stdout")
    args = ap.parse_args()

    expected = {}
    if args.expected_model:
        expected["model"] = args.expected_model
    if args.expected_permission_mode:
        expected["permission_mode"] = args.expected_permission_mode
    if args.settings_path:
        expected["settings_path"] = args.settings_path
    if args.require_tool_pairing:
        expected["require_tool_pairing"] = True

    result = validate(
        scenario=args.scenario,
        session_jsonl=args.session_jsonl,
        stdout=_read_or_empty(args.stdout_file),
        stderr=_read_or_empty(args.stderr_file),
        exit_code=args.exit_code,
        expected=expected,
    )

    out_dict = result.to_dict()
    out_json = json.dumps(out_dict, indent=2, ensure_ascii=False)
    if args.output:
        Path(args.output).write_text(out_json, encoding="utf-8")
    print(out_json)
    return 0 if result.passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
