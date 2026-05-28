#!/usr/bin/env python3
"""W259 - draw runtime moves owned draw plans into pending queue."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def forbid(text: str, token: str, label: str, failures: list[str]) -> None:
    if token in text:
        failures.append(f"{label}: forbidden {token!r}")


def main() -> None:
    terminal = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "submit_draw_plan_value_at",
        "queue_draw_plan_value",
        "queued_owned_draw_plan",
        "queued_cloned_draw_plan",
        '"move_owned_draw_plan_into_pending_queue"',
        "terminal_draw_runtime_owned_pending_submit_value",
        "draw_runtime_queues_owned_draw_plan_without_clone_path",
        "draw_runtime_reports_borrowed_queue_clone_compatibility",
    ]:
        require(terminal, token, "draw-runtime owned pending submit", failures)

    require(
        repl,
        "submit_draw_plan_value_at(item",
        "frontend owned draw-plan submit",
        failures,
    )
    forbid(
        repl,
        "submit_draw_plan_at(&item",
        "frontend borrowed draw-plan submit",
        failures,
    )

    for token in [
        "terminal_draw_runtime_owned_pending_submit",
        "terminal_pending_draw_plan_move_on_queue",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w259_terminal_draw_runtime_owned_pending_submit_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render draw-runtime owned pending submit",
        "phase note",
        failures,
    )

    if failures:
        print("=== W259 terminal draw-runtime owned pending submit smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w259_terminal_draw_runtime_owned_pending_submit_smoke: ok")


if __name__ == "__main__":
    main()
