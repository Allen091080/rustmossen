#!/usr/bin/env python3
"""W258 - draw-plan builder borrows patch operations and lines until emit."""

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
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        'let patch_operations = patch.get("operations").and_then(Value::as_array);',
        "let patch_operation_count = patch_operations.map_or(0, Vec::len);",
        "for operation in patch_operations.into_iter().flatten()",
        'let lines = operation.get("lines").and_then(Value::as_array);',
        "let source_line_count = lines.map_or(0, Vec::len);",
        "render_draw_lines_value(lines)",
        "terminal_draw_plan_borrowed_patch_inputs_value",
        '"drawPlanBorrowedPatchInputs"',
        '"borrow_patch_operations_and_lines_until_draw_json_emit"',
    ]:
        require(terminal, token, "draw-plan borrowed patch inputs", failures)

    for token in [
        '.get("operations")\n            .and_then(Value::as_array)\n            .cloned()\n            .unwrap_or_default()',
        '.get("lines")\n        .and_then(Value::as_array)\n        .cloned()\n        .unwrap_or_default()',
    ]:
        forbid(terminal, token, "pre-draw-plan clone guard", failures)

    for token in [
        "terminal_draw_plan_borrowed_patch_inputs",
        "terminal_draw_plan_region_lines_no_preclone",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w258_terminal_draw_plan_borrowed_patch_inputs_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render draw-plan borrowed patch inputs",
        "phase note",
        failures,
    )

    if failures:
        print("=== W258 terminal draw-plan borrowed patch inputs smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w258_terminal_draw_plan_borrowed_patch_inputs_smoke: ok")


if __name__ == "__main__":
    main()
