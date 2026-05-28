#!/usr/bin/env python3
"""M4.4 - current Rust terminal status/context-window accuracy smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="M4.4",
        script_name=Path(__file__).name,
        steps=[
            Step(
                "terminal_context_window_source",
                cargo_test(
                    "-p",
                    "mossen-utils",
                    "--lib",
                    "context::tests::terminal_context_window_tokens_uses_one_status_source",
                ),
                ("test result: ok.", "1 passed;"),
            ),
            Step(
                "stream_json_status_bar_reports_context",
                cargo_test(
                    "-p",
                    "mossen-cli",
                    "--bin",
                    "mossen",
                    "stream_json_render_events::tests::terminal_status_bar_reports_model_mode_reasoning_and_context",
                ),
                ("test result: ok.", "1 passed;"),
            ),
            Step(
                "tui_footer_reports_estimated_context",
                cargo_test(
                    "-p",
                    "mossen-tui",
                    "--lib",
                    "engine_stream_tests::footer_reports_estimated_context_usage_from_model_history",
                ),
                ("test result: ok.", "1 passed;"),
            ),
        ],
        checks=[
            source_check(
                "stream_json_uses_shared_context_window_source",
                "crates/mossen-cli/src/stream_json_render_events.rs",
                [
                    "use mossen_utils::context::terminal_context_window_tokens;",
                    "terminal_context_window_tokens(model)",
                ],
            ),
            source_check(
                "tui_uses_shared_context_window_source",
                "crates/mossen-tui/src/app.rs",
                [
                    "mossen_utils::context::terminal_context_window_tokens(model)",
                    "context_window_tokens_for_footer",
                ],
            ),
        ],
        design_note=(
            "M4.4 validates ctx/window accuracy for terminal surfaces. "
            "stream-json status and the TUI footer now share the same "
            "mossen-utils context-window source, including override and "
            "custom-backend precedence."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
