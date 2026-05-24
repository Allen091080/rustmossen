#!/usr/bin/env python3
"""W171 - terminal-render Ctrl+C interrupt smoke."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def require_compact(text: str, token: str, label: str, failures: list[str]) -> None:
    compact_text = "".join(text.split())
    compact_token = "".join(token.split())
    if compact_token not in compact_text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    types = (ROOT / "crates/mossen-agent/src/types.rs").read_text()
    engine = (ROOT / "crates/mossen-agent/src/engine.rs").read_text()
    events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "pub cancel_token: Option<CancellationToken>",
        "cancel_token: params.cancel_token",
    ]:
        require(types + engine, token, "agent cancellation plumbing", failures)
    require_compact(
        types + engine,
        "self.cancel = options.cancel_token.clone().unwrap_or_else(CancellationToken::new)",
        "agent cancellation plumbing",
        failures,
    )

    for token in [
        "TerminalRenderFrontendEvent::Interrupt",
        "terminal_cancel_token",
        "prompt_params.cancel_token = Some(terminal_cancel_token.clone())",
        "terminal_render_handle_interrupt",
        "cancel_token.cancel()",
        "cancel_pending_permission",
        "PermissionDecision::Deny",
        "terminal: \"cancelled\"",
        "maps_ctrl_c_to_interrupt_even_during_edit_capture",
        "terminal_approval_bridge_interrupt_denies_pending_permission",
    ]:
        require(repl, token, "terminal-render interrupt bridge", failures)

    require(events, '"interrupted" => "turn interrupted"', "render interrupt label", failures)

    for token in [
        "terminal_frontend_ctrl_c_interrupt",
        "terminal_frontend_interrupt_cancels_turn",
        "terminal_frontend_interrupt_unblocks_approval",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w171_terminal_render_interrupt_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render Ctrl-C interrupt bridge",
        "phase note",
        failures,
    )

    if failures:
        print("=== W171 terminal-render interrupt smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w171_terminal_render_interrupt_smoke: ok")


if __name__ == "__main__":
    main()
