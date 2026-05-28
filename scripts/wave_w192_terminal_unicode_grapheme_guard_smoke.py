#!/usr/bin/env python3
"""W192 - terminal renderer keeps complex Unicode graphemes intact."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    cargo = (ROOT / "crates/mossen-cli/Cargo.toml").read_text()
    renderer = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    require(cargo, "unicode-segmentation.workspace = true", "cli dependency", failures)

    for token in [
        "UnicodeSegmentation::graphemes",
        "terminal_safe_grapheme",
        "graphemeClusterSafe",
        "graphemeClusterWidthGuard",
        "terminal_bounded_line_preserves_complex_unicode_graphemes",
        "terminal_soft_wrap_preserves_complex_unicode_graphemes",
    ]:
        require(renderer, token, "unicode grapheme renderer", failures)

    for token in [
        "terminal_unicode_grapheme_cluster_guard",
        "terminal_complex_unicode_width_guard",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w192_terminal_unicode_grapheme_guard_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render Unicode grapheme width guard",
        "phase note",
        failures,
    )

    if failures:
        print("=== W192 terminal unicode grapheme guard smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w192_terminal_unicode_grapheme_guard_smoke: ok")


if __name__ == "__main__":
    main()
