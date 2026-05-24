#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from wave_w295_terminal_manual_scroll_approval_approve_pty_smoke import (
    COMMAND_STARTED_SENTINEL_PATH,
    SENTINEL_PATH,
    run_manual_scroll_approval_approve_smoke,
)


@dataclass(frozen=True)
class EndReleaseCase:
    name: str
    fixture_name: str
    marker: str
    sleep_secs: str
    mouse_scroll: bool
    resize: bool
    release_delay_secs: str


CASES = [
    EndReleaseCase(
        name="mouse_end",
        fixture_name="W319_terminal_mouse_command_end_live_tail_release_after_approval_pty_smoke",
        marker="TERMINAL_APPROVAL_MOUSE_COMMAND_END_RELEASE_W319_%03d",
        sleep_secs="0.45",
        mouse_scroll=True,
        resize=False,
        release_delay_secs="0.14",
    ),
    EndReleaseCase(
        name="resize_end",
        fixture_name="W319_terminal_resize_command_end_live_tail_release_after_approval_pty_smoke",
        marker="TERMINAL_APPROVAL_RESIZE_COMMAND_END_RELEASE_W319_%03d",
        sleep_secs="0.6",
        mouse_scroll=False,
        resize=True,
        release_delay_secs="0.3",
    ),
    EndReleaseCase(
        name="mouse_resize_end",
        fixture_name="W319_terminal_mouse_resize_command_end_live_tail_release_after_approval_pty_smoke",
        marker="TERMINAL_APPROVAL_MOUSE_RESIZE_COMMAND_END_RELEASE_W319_%03d",
        sleep_secs="0.6",
        mouse_scroll=True,
        resize=True,
        release_delay_secs="0.3",
    ),
]

MUTATED_ENV_KEYS = [
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_FIXTURE_NAME",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_KEY",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND_SCROLL_DELAY_SECS",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND_RESIZE_DELAY_SECS",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND_RELEASE_DELAY_SECS",
]


def live_tail_command(case: EndReleaseCase) -> str:
    return (
        ": 'TERMINAL_APPROVAL_APPROVE_COMMAND_%s'; "
        f"touch {COMMAND_STARTED_SENTINEL_PATH}; "
        f"sleep {case.sleep_secs}; "
        "printf 'TERMINAL_APPROVAL_APPROVE_COMMAND_%s\\n' W295; "
        "for idx in $(seq 0 79); do "
        f"printf '{case.marker}\\n' \"$idx\"; "
        "done; "
        f"touch {SENTINEL_PATH}; "
        "sleep 0.2"
    )


def set_case_env(case: EndReleaseCase) -> None:
    os.environ["MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_FIXTURE_NAME"] = case.fixture_name
    os.environ["MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND"] = live_tail_command(case)
    os.environ[
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE"
    ] = "0" if case.mouse_scroll else "1"
    os.environ[
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE"
    ] = "1" if case.mouse_scroll else "0"
    os.environ[
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE"
    ] = "1" if case.resize else "0"
    os.environ[
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE"
    ] = "1"
    os.environ["MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_KEY"] = "end"
    os.environ["MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND_SCROLL_DELAY_SECS"] = "0.08"
    os.environ["MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND_RESIZE_DELAY_SECS"] = "0.08"
    os.environ[
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND_RELEASE_DELAY_SECS"
    ] = case.release_delay_secs


def run_case(case: EndReleaseCase) -> dict[str, Any]:
    set_case_env(case)
    result = run_manual_scroll_approval_approve_smoke()
    actions = result.get("actions") or []
    release_actions = [
        action
        for action in actions
        if action.get("name") == "end_live_tail_release_during_command_after_approve"
    ]
    diagnostics = result.get("diagnostics") or {}
    extra_assertions = [
        (
            "end_release_action_recorded",
            bool(release_actions),
            str(actions),
        ),
        (
            "end_release_key_reported",
            result.get("release_key_during_command_after_approve") == "end",
            str(result.get("release_key_during_command_after_approve")),
        ),
        (
            "end_release_hid_stdout_before_restore",
            result.get("command_output_visible_before_command_release") is False,
            str(result.get("command_output_visible_before_command_release")),
        ),
        (
            "end_release_command_stdout_rendered",
            result.get("command_stdout_rendered") is True,
            str(result.get("command_stdout_rendered")),
        ),
        (
            "mouse_capture_balanced_when_required",
            (not case.mouse_scroll)
            or (
                result.get("mouse_enable_count", 0) > 0
                and result.get("mouse_enable_count") == result.get("mouse_disable_count")
            ),
            f"mouse={result.get('mouse_enable_count')}/{result.get('mouse_disable_count')}",
        ),
        (
            "resize_finished_on_latest_viewport_when_required",
            (not case.resize) or diagnostics.get("lastExecutionViewportColumns") == 118,
            str(diagnostics),
        ),
    ]
    return {
        "case": case.name,
        "ok": result.get("ok") is True
        and all(passed for _, passed, _ in extra_assertions),
        "fixture_root": result.get("fixture_root"),
        "extra_assertions": [
            {
                "name": name,
                "passed": passed,
                "evidence": evidence,
            }
            for name, passed, evidence in extra_assertions
        ],
        "result": result,
    }


def main() -> int:
    saved_env = {key: os.environ.get(key) for key in MUTATED_ENV_KEYS}
    results = []
    try:
        for case in CASES:
            results.append(run_case(case))
    finally:
        for key, value in saved_env.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value

    aggregate = {
        "ok": all(result["ok"] for result in results),
        "cases": results,
    }
    print(json.dumps(aggregate, indent=2, ensure_ascii=False))
    return 0 if aggregate["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
