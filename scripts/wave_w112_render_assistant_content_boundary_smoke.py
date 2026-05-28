#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
LIFECYCLE = ROOT / "crates/mossen-tui/src/render_lifecycle.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def reject(text: str, needle: str, label: str) -> None:
    if needle in text:
        raise SystemExit(f"forbidden {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    lifecycle = LIFECYCLE.read_text()
    run_all = RUN_ALL.read_text()

    require(
        app,
        "let content_facts = assistant_content_facts(&message);",
        "assistant content facts call",
    )
    require(
        app,
        "for tool_use in content_facts.tool_uses",
        "assistant tool-use facts loop",
    )
    for forbidden in [
        "let mut full_text = String::new();",
        "let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new();",
        "ContentBlock::ToolUse(tu) =>",
    ]:
        reject(app, forbidden, "root assistant content block extraction")

    require(
        lifecycle,
        "pub fn assistant_content_facts",
        "layer1 assistant content facts helper",
    )
    require(
        lifecycle,
        "pub struct AssistantToolUseFacts",
        "layer1 assistant tool-use facts type",
    )
    require(
        lifecycle,
        "assistant_content_facts_extract_text_and_tool_uses_once",
        "assistant content facts regression",
    )
    require(
        run_all,
        "wave_w112_render_assistant_content_boundary_smoke.py",
        "run_all registration",
    )

    print("wave_w112_render_assistant_content_boundary_smoke: ok")


if __name__ == "__main__":
    main()
