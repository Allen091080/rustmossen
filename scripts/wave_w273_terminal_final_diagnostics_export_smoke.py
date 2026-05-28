#!/usr/bin/env python3
"""W273 - terminal-render exports final diagnostics JSON on request."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "MOSSEN_TERMINAL_RENDER_DIAGNOSTICS_PATH",
        "terminal_render_export_final_diagnostics_if_requested",
        "terminal_render_write_diagnostics_snapshot_to_path",
        "runtime_diagnostics_value",
        "serde_json::to_vec_pretty",
        "terminal_render_writes_final_diagnostics_snapshot_to_requested_path",
    ]:
        require(repl, token, "final diagnostics export", failures)

    for token in [
        "terminal_render_final_diagnostics_export_env",
        "terminal_render_final_diagnostics_json_file",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w273_terminal_final_diagnostics_export_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render final diagnostics export",
        "phase note",
        failures,
    )

    if failures:
        print("=== W273 terminal final diagnostics export smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w273_terminal_final_diagnostics_export_smoke: ok")


if __name__ == "__main__":
    main()
