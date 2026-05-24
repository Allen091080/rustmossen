#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    structured = read("crates/mossen-cli/src/structured_io.rs")
    capabilities = read("crates/mossen-agent/src/services/root/slash_command_capabilities.rs")
    run_all = read("scripts/run_all_smoke.sh")
    phase = read("phases/03g-rendering-product-grade-plan.md")

    for needle in [
        "async fn slash_diff_response",
        '"command": "diff"',
        '"rawDiffIncluded": false',
        '"contentIncluded": false',
        '"filesTruncated"',
        'fetch_git_diff(&cwd).await',
        '"diff"',
        "slash_command_diff_returns_bounded_git_summary",
        '"command":"/changes"',
    ]:
        require(structured, needle, "structured diff slash command")

    for needle in [
        "ResultKind::Diff",
        '"slash.diff"',
        "CommandStatus::Available",
        '"changes".to_string()',
        '"summary".to_string()',
        "bounded read-only git diff summary",
    ]:
        require(capabilities, needle, "diff capability")

    require(
        run_all,
        "wave_w208_stream_json_diff_slash_command_smoke",
        "run_all registration",
    )
    require(phase, "2026-05-23 Stream-json /diff slash command bridge", "phase note")

    print("wave_w208_stream_json_diff_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
