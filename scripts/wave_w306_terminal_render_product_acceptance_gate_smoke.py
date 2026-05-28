#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


REQUIRED_SMOKES = [
    "wave_w288_terminal_manual_scroll_completion_hold_smoke",
    "wave_w289_terminal_manual_scroll_tail_hold_pty_smoke",
    "wave_w290_terminal_manual_scroll_teardown_release_pty_smoke",
    "wave_w291_terminal_manual_scroll_resize_teardown_release_pty_smoke",
    "wave_w292_terminal_manual_scroll_resize_interrupt_pty_smoke",
    "wave_w293_terminal_manual_scroll_approval_bypass_pty_smoke",
    "wave_w294_terminal_manual_scroll_approval_reject_pty_smoke",
    "wave_w295_terminal_manual_scroll_approval_approve_pty_smoke",
    "wave_w296_terminal_manual_scroll_approval_edit_command_pty_smoke",
    "wave_w297_terminal_manual_scroll_approval_always_allow_pty_smoke",
    "wave_w298_terminal_manual_scroll_approval_edit_cancel_pty_smoke",
    "wave_w299_terminal_manual_scroll_approval_resize_approve_pty_smoke",
    "wave_w300_terminal_manual_scroll_approval_active_scroll_reject_pty_smoke",
    "wave_w301_terminal_mouse_scroll_approval_reject_pty_smoke",
    "wave_w302_terminal_mouse_scroll_approval_approve_pty_smoke",
    "wave_w303_terminal_mouse_scroll_approval_edit_command_pty_smoke",
    "wave_w304_terminal_mouse_scroll_approval_always_allow_pty_smoke",
    "wave_w305_terminal_manual_scroll_command_output_after_approval_pty_smoke",
    "wave_w307_terminal_mouse_scroll_command_output_after_approval_pty_smoke",
    "wave_w308_terminal_manual_scroll_command_output_resize_after_approval_pty_smoke",
    "wave_w309_terminal_mouse_scroll_command_output_resize_after_approval_pty_smoke",
    "wave_w310_terminal_manual_scroll_command_interrupt_after_approval_pty_smoke",
    "wave_w311_terminal_mouse_scroll_command_interrupt_after_approval_pty_smoke",
    "wave_w312_terminal_manual_scroll_command_resize_interrupt_after_approval_pty_smoke",
    "wave_w313_terminal_mouse_scroll_command_resize_interrupt_after_approval_pty_smoke",
    "wave_w314_terminal_manual_scroll_command_live_tail_release_after_approval_pty_smoke",
    "wave_w315_terminal_mouse_scroll_command_live_tail_release_after_approval_pty_smoke",
    "wave_w316_terminal_manual_scroll_command_resize_live_tail_release_after_approval_pty_smoke",
    "wave_w317_terminal_mouse_scroll_command_resize_live_tail_release_after_approval_pty_smoke",
    "wave_w318_terminal_manual_scroll_command_end_live_tail_release_after_approval_pty_smoke",
    "wave_w319_terminal_command_end_live_tail_matrix_after_approval_pty_smoke",
    "wave_w320_terminal_no_fullscreen_clear_external_pty_contract_smoke",
    "wave_w321_terminal_command_pagedown_live_tail_matrix_after_approval_pty_smoke",
    "wave_w322_terminal_command_mouse_wheel_down_live_tail_matrix_after_approval_pty_smoke",
]

REQUIRED_STATUS_TOKENS = [
    "terminal_render_completion_manual_scroll_hold",
    "terminal_render_manual_scroll_tail_hold_pty_smoke",
    "terminal_render_external_process_tail_hold_until_restore",
    "terminal_render_manual_scroll_teardown_release_pty_smoke",
    "terminal_render_external_process_teardown_release_no_stuck_pending",
    "terminal_render_manual_scroll_resize_teardown_release_pty_smoke",
    "terminal_render_external_process_resize_teardown_latest_viewport",
    "terminal_render_manual_scroll_resize_interrupt_pty_smoke",
    "terminal_render_external_process_resize_interrupt_latest_viewport",
    "terminal_render_manual_scroll_approval_bypass_pty_smoke",
    "terminal_render_external_process_approval_bypasses_manual_scroll",
    "terminal_render_manual_scroll_approval_reject_pty_smoke",
    "terminal_render_external_process_approval_reject_no_execute",
    "terminal_render_manual_scroll_approval_approve_pty_smoke",
    "terminal_render_external_process_approval_approve_executes_and_renders",
    "terminal_render_manual_scroll_approval_always_allow_pty_smoke",
    "terminal_render_external_process_approval_always_allow_executes_and_renders",
    "terminal_render_manual_scroll_approval_edit_command_pty_smoke",
    "terminal_render_external_process_approval_edit_command_executes_updated_input",
    "terminal_render_manual_scroll_approval_edit_cancel_pty_smoke",
    "terminal_render_external_process_approval_edit_cancel_rejects_without_execute",
    "terminal_render_manual_scroll_approval_resize_approve_pty_smoke",
    "terminal_render_external_process_approval_resize_approve_latest_viewport",
    "terminal_render_manual_scroll_approval_active_scroll_reject_pty_smoke",
    "terminal_render_external_process_approval_survives_active_scroll_reject",
    "terminal_render_mouse_scroll_approval_reject_pty_smoke",
    "terminal_render_external_process_approval_mouse_scroll_reject",
    "terminal_render_mouse_scroll_approval_approve_pty_smoke",
    "terminal_render_external_process_approval_mouse_scroll_approve_executes",
    "terminal_render_mouse_scroll_approval_edit_command_pty_smoke",
    "terminal_render_external_process_approval_mouse_scroll_edit_command_executes_updated_input",
    "terminal_render_mouse_scroll_approval_always_allow_pty_smoke",
    "terminal_render_external_process_approval_mouse_scroll_always_allow_executes",
    "terminal_render_manual_scroll_command_output_after_approval_pty_smoke",
    "terminal_render_external_process_command_output_manual_scroll_hold_after_approval",
    "terminal_render_mouse_scroll_command_output_after_approval_pty_smoke",
    "terminal_render_external_process_command_output_mouse_scroll_hold_after_approval",
    "terminal_render_manual_scroll_command_output_resize_after_approval_pty_smoke",
    "terminal_render_external_process_command_output_resize_hold_after_approval",
    "terminal_render_mouse_scroll_command_output_resize_after_approval_pty_smoke",
    "terminal_render_external_process_command_output_mouse_resize_hold_after_approval",
    "terminal_render_manual_scroll_command_interrupt_after_approval_pty_smoke",
    "terminal_render_external_process_command_interrupt_manual_scroll_after_approval",
    "terminal_render_mouse_scroll_command_interrupt_after_approval_pty_smoke",
    "terminal_render_external_process_command_interrupt_mouse_scroll_after_approval",
    "terminal_render_manual_scroll_command_resize_interrupt_after_approval_pty_smoke",
    "terminal_render_external_process_command_resize_interrupt_manual_scroll_after_approval",
    "terminal_render_mouse_scroll_command_resize_interrupt_after_approval_pty_smoke",
    "terminal_render_external_process_command_resize_interrupt_mouse_scroll_after_approval",
    "terminal_render_manual_scroll_command_live_tail_release_after_approval_pty_smoke",
    "terminal_render_external_process_command_live_tail_release_after_approval",
    "terminal_render_mouse_scroll_command_live_tail_release_after_approval_pty_smoke",
    "terminal_render_external_process_command_mouse_live_tail_release_after_approval",
    "terminal_render_manual_scroll_command_resize_live_tail_release_after_approval_pty_smoke",
    "terminal_render_external_process_command_resize_live_tail_release_after_approval",
    "terminal_render_mouse_scroll_command_resize_live_tail_release_after_approval_pty_smoke",
    "terminal_render_external_process_command_mouse_resize_live_tail_release_after_approval",
    "terminal_render_manual_scroll_command_end_live_tail_release_after_approval_pty_smoke",
    "terminal_render_external_process_command_end_live_tail_release_after_approval",
    "terminal_render_command_end_live_tail_matrix_after_approval_pty_smoke",
    "terminal_render_external_process_command_end_live_tail_release_after_approval_matrix",
    "terminal_render_external_process_no_fullscreen_clear",
    "terminal_render_external_process_no_fullscreen_clear_pty_contract_smoke",
    "terminal_render_product_external_pty_no_fullscreen_clear_w104_w320",
    "terminal_render_command_pagedown_live_tail_matrix_after_approval_pty_smoke",
    "terminal_render_external_process_command_pagedown_live_tail_release_after_approval_matrix",
    "terminal_render_command_mouse_wheel_down_live_tail_matrix_after_approval_pty_smoke",
    "terminal_render_external_process_command_mouse_wheel_down_live_tail_release_after_approval_matrix",
    "terminal_render_product_external_pty_matrix_w288_w307",
    "terminal_render_product_external_pty_matrix_w288_w308",
    "terminal_render_product_external_pty_matrix_w288_w309",
    "terminal_render_product_external_pty_matrix_w288_w310",
    "terminal_render_product_external_pty_matrix_w288_w311",
    "terminal_render_product_external_pty_matrix_w288_w312",
    "terminal_render_product_external_pty_matrix_w288_w313",
    "terminal_render_product_external_pty_matrix_w288_w314",
    "terminal_render_product_external_pty_matrix_w288_w315",
    "terminal_render_product_external_pty_matrix_w288_w316",
    "terminal_render_product_external_pty_matrix_w288_w317",
    "terminal_render_product_external_pty_matrix_w288_w318",
    "terminal_render_product_external_pty_matrix_w288_w319",
    "terminal_render_product_external_pty_matrix_w288_w321",
    "terminal_render_product_external_pty_matrix_w288_w322",
]

REQUIRED_W295_TOKENS = [
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "page_up_manual_scroll_during_command_after_approve",
    "mouse_wheel_up_manual_scroll_during_command_after_approve",
    "command_completed_while_manual_scroll_after_approve",
    "command_result_hidden_while_manual_scroll_after_approve",
    "command_result_flushed_after_manual_scroll_teardown_release",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "resize_narrow_during_command_after_approve",
    "resize_wide_during_command_after_approve",
    "command_resize_kept_output_hidden_until_release",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_INTERRUPT_DURING_COMMAND_AFTER_APPROVE",
    "ctrl_c_interrupt_during_command_after_approve",
    "command_interrupt_held_output_until_cancel",
    "command_cancelled_before_completion",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_KEY",
    "ctrl_l_live_tail_release_during_command_after_approve",
    "end_live_tail_release_during_command_after_approve",
    "pagedown_live_tail_release_during_command_after_approve",
    "mouse_wheel_down_live_tail_release_during_command_after_approve",
    "command_release_uses_mouse",
    "command_result_flushed_after_manual_scroll_live_tail_release",
    "manualScrollTeardownReleaseCount",
]

REQUIRED_W305_TOKENS = [
    "TERMINAL_APPROVAL_COMMAND_SCROLL_W305_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "W305_terminal_manual_scroll_command_output_after_approval_pty_smoke",
]

REQUIRED_W307_TOKENS = [
    "TERMINAL_APPROVAL_MOUSE_COMMAND_SCROLL_W307_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "W307_terminal_mouse_scroll_command_output_after_approval_pty_smoke",
]

REQUIRED_W308_TOKENS = [
    "TERMINAL_APPROVAL_RESIZE_COMMAND_SCROLL_W308_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "W308_terminal_manual_scroll_command_output_resize_after_approval_pty_smoke",
]

REQUIRED_W309_TOKENS = [
    "TERMINAL_APPROVAL_MOUSE_RESIZE_COMMAND_SCROLL_W309_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "W309_terminal_mouse_scroll_command_output_resize_after_approval_pty_smoke",
]

REQUIRED_W310_TOKENS = [
    "COMMAND_STARTED_SENTINEL_PATH",
    "TERMINAL_APPROVAL_COMMAND_INTERRUPT_W310_SHOULD_NOT_RENDER",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_INTERRUPT_DURING_COMMAND_AFTER_APPROVE",
    "W310_terminal_manual_scroll_command_interrupt_after_approval_pty_smoke",
]

REQUIRED_W311_TOKENS = [
    "COMMAND_STARTED_SENTINEL_PATH",
    "TERMINAL_APPROVAL_MOUSE_COMMAND_INTERRUPT_W311_SHOULD_NOT_RENDER",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_INTERRUPT_DURING_COMMAND_AFTER_APPROVE",
    "W311_terminal_mouse_scroll_command_interrupt_after_approval_pty_smoke",
]

REQUIRED_W312_TOKENS = [
    "COMMAND_STARTED_SENTINEL_PATH",
    "TERMINAL_APPROVAL_RESIZE_COMMAND_INTERRUPT_W312_SHOULD_NOT_RENDER",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_INTERRUPT_DURING_COMMAND_AFTER_APPROVE",
    "W312_terminal_manual_scroll_command_resize_interrupt_after_approval_pty_smoke",
]

REQUIRED_W313_TOKENS = [
    "COMMAND_STARTED_SENTINEL_PATH",
    "TERMINAL_APPROVAL_MOUSE_RESIZE_COMMAND_INTERRUPT_W313_SHOULD_NOT_RENDER",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_INTERRUPT_DURING_COMMAND_AFTER_APPROVE",
    "W313_terminal_mouse_scroll_command_resize_interrupt_after_approval_pty_smoke",
]

REQUIRED_W314_TOKENS = [
    "COMMAND_STARTED_SENTINEL_PATH",
    "TERMINAL_APPROVAL_COMMAND_RELEASE_W314_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE",
    "W314_terminal_manual_scroll_command_live_tail_release_after_approval_pty_smoke",
]

REQUIRED_W315_TOKENS = [
    "COMMAND_STARTED_SENTINEL_PATH",
    "TERMINAL_APPROVAL_MOUSE_COMMAND_RELEASE_W315_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE",
    "W315_terminal_mouse_scroll_command_live_tail_release_after_approval_pty_smoke",
]

REQUIRED_W316_TOKENS = [
    "COMMAND_STARTED_SENTINEL_PATH",
    "TERMINAL_APPROVAL_RESIZE_COMMAND_RELEASE_W316_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE",
    "W316_terminal_manual_scroll_command_resize_live_tail_release_after_approval_pty_smoke",
]

REQUIRED_W317_TOKENS = [
    "COMMAND_STARTED_SENTINEL_PATH",
    "TERMINAL_APPROVAL_MOUSE_RESIZE_COMMAND_RELEASE_W317_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE",
    "W317_terminal_mouse_scroll_command_resize_live_tail_release_after_approval_pty_smoke",
]

REQUIRED_W318_TOKENS = [
    "COMMAND_STARTED_SENTINEL_PATH",
    "TERMINAL_APPROVAL_COMMAND_END_RELEASE_W318_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_KEY",
    "W318_terminal_manual_scroll_command_end_live_tail_release_after_approval_pty_smoke",
]

REQUIRED_W319_TOKENS = [
    "EndReleaseCase",
    "TERMINAL_APPROVAL_MOUSE_COMMAND_END_RELEASE_W319_%03d",
    "TERMINAL_APPROVAL_RESIZE_COMMAND_END_RELEASE_W319_%03d",
    "TERMINAL_APPROVAL_MOUSE_RESIZE_COMMAND_END_RELEASE_W319_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_KEY",
    "end_live_tail_release_during_command_after_approve",
    "mouse_capture_balanced_when_required",
    "resize_finished_on_latest_viewport_when_required",
]

REQUIRED_W320_TOKENS = [
    "PTY_SCRIPTS",
    "wave_w104_render_pty_live_streaming_soak.py",
    "wave_w298_terminal_manual_scroll_approval_edit_cancel_pty_smoke.py",
    "full_clears == 0",
    "full_clears <= 2",
    "legacy fullscreen-clear allowance still present",
    "terminal_render_external_process_no_fullscreen_clear",
    "terminal_render_external_process_no_fullscreen_clear_pty_contract_smoke",
]

REQUIRED_W321_TOKENS = [
    "PageDownReleaseCase",
    "TERMINAL_APPROVAL_MANUAL_COMMAND_PAGEDOWN_RELEASE_W321_%03d",
    "TERMINAL_APPROVAL_MOUSE_COMMAND_PAGEDOWN_RELEASE_W321_%03d",
    "TERMINAL_APPROVAL_RESIZE_COMMAND_PAGEDOWN_RELEASE_W321_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_KEY",
    "pagedown_live_tail_release_during_command_after_approve",
    "mouse_capture_balanced_when_required",
    "resize_finished_on_latest_viewport_when_required",
]

REQUIRED_W322_TOKENS = [
    "MouseWheelDownReleaseCase",
    "TERMINAL_APPROVAL_MANUAL_COMMAND_MOUSE_DOWN_RELEASE_W322_%03d",
    "TERMINAL_APPROVAL_MOUSE_COMMAND_MOUSE_DOWN_RELEASE_W322_%03d",
    "TERMINAL_APPROVAL_RESIZE_COMMAND_MOUSE_DOWN_RELEASE_W322_%03d",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_MOUSE_SCROLL_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE",
    "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_KEY",
    "mouse_wheel_down_live_tail_release_during_command_after_approve",
    "mouse_wheel_down_release_uses_mouse_capture",
    "mouse_capture_balanced_when_required",
    "resize_finished_on_latest_viewport_when_required",
]


def require(condition: bool, label: str, failures: list[str]) -> None:
    if not condition:
        failures.append(label)


def require_text(text: str, token: str, label: str, failures: list[str]) -> None:
    require(token in text, f"{label}: missing {token!r}", failures)


def main() -> int:
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    w295 = (
        ROOT / "scripts/wave_w295_terminal_manual_scroll_approval_approve_pty_smoke.py"
    ).read_text()
    w305 = (
        ROOT
        / "scripts/wave_w305_terminal_manual_scroll_command_output_after_approval_pty_smoke.py"
    ).read_text()
    w307 = (
        ROOT
        / "scripts/wave_w307_terminal_mouse_scroll_command_output_after_approval_pty_smoke.py"
    ).read_text()
    w308 = (
        ROOT
        / "scripts/wave_w308_terminal_manual_scroll_command_output_resize_after_approval_pty_smoke.py"
    ).read_text()
    w309 = (
        ROOT
        / "scripts/wave_w309_terminal_mouse_scroll_command_output_resize_after_approval_pty_smoke.py"
    ).read_text()
    w310 = (
        ROOT
        / "scripts/wave_w310_terminal_manual_scroll_command_interrupt_after_approval_pty_smoke.py"
    ).read_text()
    w311 = (
        ROOT
        / "scripts/wave_w311_terminal_mouse_scroll_command_interrupt_after_approval_pty_smoke.py"
    ).read_text()
    w312 = (
        ROOT
        / "scripts/wave_w312_terminal_manual_scroll_command_resize_interrupt_after_approval_pty_smoke.py"
    ).read_text()
    w313 = (
        ROOT
        / "scripts/wave_w313_terminal_mouse_scroll_command_resize_interrupt_after_approval_pty_smoke.py"
    ).read_text()
    w314 = (
        ROOT
        / "scripts/wave_w314_terminal_manual_scroll_command_live_tail_release_after_approval_pty_smoke.py"
    ).read_text()
    w315 = (
        ROOT
        / "scripts/wave_w315_terminal_mouse_scroll_command_live_tail_release_after_approval_pty_smoke.py"
    ).read_text()
    w316 = (
        ROOT
        / "scripts/wave_w316_terminal_manual_scroll_command_resize_live_tail_release_after_approval_pty_smoke.py"
    ).read_text()
    w317 = (
        ROOT
        / "scripts/wave_w317_terminal_mouse_scroll_command_resize_live_tail_release_after_approval_pty_smoke.py"
    ).read_text()
    w318 = (
        ROOT
        / "scripts/wave_w318_terminal_manual_scroll_command_end_live_tail_release_after_approval_pty_smoke.py"
    ).read_text()
    w319 = (
        ROOT
        / "scripts/wave_w319_terminal_command_end_live_tail_matrix_after_approval_pty_smoke.py"
    ).read_text()
    w320 = (
        ROOT
        / "scripts/wave_w320_terminal_no_fullscreen_clear_external_pty_contract_smoke.py"
    ).read_text()
    w321 = (
        ROOT
        / "scripts/wave_w321_terminal_command_pagedown_live_tail_matrix_after_approval_pty_smoke.py"
    ).read_text()
    w322 = (
        ROOT
        / "scripts/wave_w322_terminal_command_mouse_wheel_down_live_tail_matrix_after_approval_pty_smoke.py"
    ).read_text()

    failures: list[str] = []
    for smoke in REQUIRED_SMOKES:
        require(
            (ROOT / "scripts" / f"{smoke}.py").exists(),
            f"smoke file exists: {smoke}",
            failures,
        )
        require_text(run_all, smoke, "run_all registration", failures)

    for token in REQUIRED_STATUS_TOKENS:
        require_text(structured, token, "status metadata", failures)

    for token in REQUIRED_W295_TOKENS:
        require_text(w295, token, "W295 extensible approval harness", failures)

    for token in REQUIRED_W305_TOKENS:
        require_text(w305, token, "W305 command-output hold wrapper", failures)

    for token in REQUIRED_W307_TOKENS:
        require_text(w307, token, "W307 mouse command-output hold wrapper", failures)

    for token in REQUIRED_W308_TOKENS:
        require_text(w308, token, "W308 resize command-output hold wrapper", failures)

    for token in REQUIRED_W309_TOKENS:
        require_text(w309, token, "W309 mouse resize command-output hold wrapper", failures)

    for token in REQUIRED_W310_TOKENS:
        require_text(w310, token, "W310 command interrupt hold wrapper", failures)

    for token in REQUIRED_W311_TOKENS:
        require_text(w311, token, "W311 mouse command interrupt hold wrapper", failures)

    for token in REQUIRED_W312_TOKENS:
        require_text(w312, token, "W312 resize command interrupt hold wrapper", failures)

    for token in REQUIRED_W313_TOKENS:
        require_text(
            w313,
            token,
            "W313 mouse resize command interrupt hold wrapper",
            failures,
        )

    for token in REQUIRED_W314_TOKENS:
        require_text(
            w314,
            token,
            "W314 command live-tail release wrapper",
            failures,
        )

    for token in REQUIRED_W315_TOKENS:
        require_text(
            w315,
            token,
            "W315 mouse command live-tail release wrapper",
            failures,
        )

    for token in REQUIRED_W316_TOKENS:
        require_text(
            w316,
            token,
            "W316 resize command live-tail release wrapper",
            failures,
        )

    for token in REQUIRED_W317_TOKENS:
        require_text(
            w317,
            token,
            "W317 mouse resize command live-tail release wrapper",
            failures,
        )

    for token in REQUIRED_W318_TOKENS:
        require_text(
            w318,
            token,
            "W318 command End live-tail release wrapper",
            failures,
        )

    for token in REQUIRED_W319_TOKENS:
        require_text(
            w319,
            token,
            "W319 command End live-tail matrix wrapper",
            failures,
        )

    for token in REQUIRED_W320_TOKENS:
        require_text(
            w320,
            token,
            "W320 no-fullscreen-clear contract wrapper",
            failures,
        )

    for token in REQUIRED_W321_TOKENS:
        require_text(
            w321,
            token,
            "W321 command PageDown live-tail matrix wrapper",
            failures,
        )

    for token in REQUIRED_W322_TOKENS:
        require_text(
            w322,
            token,
            "W322 command mouse-wheel-down live-tail matrix wrapper",
            failures,
        )

    require_text(
        run_all,
        "wave_w306_terminal_render_product_acceptance_gate_smoke",
        "run_all self-registration",
        failures,
    )
    require_text(
        structured,
        "terminal_render_product_acceptance_gate_smoke",
        "status metadata",
        failures,
    )
    if failures:
        print("=== W306 terminal render product acceptance gate smoke ===")
        for failure in failures:
            print(f"- {failure}")
        return 1
    print("wave_w306_terminal_render_product_acceptance_gate_smoke: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
