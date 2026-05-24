#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    executor = read("crates/mossen-skills/src/executor.rs")
    skills_lib = read("crates/mossen-skills/src/lib.rs")
    skill_tool = read("crates/mossen-tools/src/skill.rs")
    app = read("crates/mossen-tui/src/app.rs")
    render_model = read("crates/mossen-tui/src/render_model.rs")
    keybinding_smoke = read("crates/mossen-tui/tests/keybinding_smoke.rs")
    render_snapshot = read("crates/mossen-tui/tests/render_snapshot.rs")
    run_all = read("scripts/run_all_smoke.sh")
    phase = read("phases/03g-rendering-product-grade-plan.md")

    for needle in [
        "pub fn format_invoked_skill_prompt",
        "mossen_utils::messages::format_command_input_tags(skill_name, args)",
        "let body = rendered_prompt.trim();",
        'format!("{tags}\\n\\n{body}")',
    ]:
        require(executor, needle, "skill prompt command-tag formatter")
    require(
        skills_lib,
        "format_invoked_skill_prompt",
        "mossen-skills public formatter export",
    )

    for needle in [
        "mossen_skills::format_invoked_skill_prompt(",
        "skill_invocation_metadata",
        '"status": "loaded"',
        '"status": "missing"',
        '"resultIncludesCommandTags": result_includes_command_tags',
        '"rawSkillRootIncluded": false',
        '"metadataContentRedacted": true',
        "skill_tool_executes_loaded_dynamic_skill",
        "skill_tool_reports_missing_skill_as_structured_error",
        "<command-name>/echoer</command-name>",
        "<command-args>from model</command-args>",
    ]:
        require(skill_tool, needle, "Skill tool command-tag handoff")

    for needle in [
        "fn try_handle_skill_command",
        "skill_invocation_transcript_message",
        "let model_prompt =",
        "format_invoked_skill_prompt(craft.name(), args_raw, &prompt)",
        "submit_prompt_to_engine(model_prompt",
    ]:
        require(app, needle, "slash skill command-tag model submission")

    for needle in [
        "strip_display_tags_allow_empty",
        "fn sanitized_skill_result_value",
        'sanitized_skill_result_value(object.get("result"))',
        "display_result.as_ref()",
        "ToolSectionKind::Output",
    ]:
        require(render_model, needle, "Skill result display tag stripping")

    for needle in [
        "slash_skill_submission_includes_command_tags_but_transcript_stays_clean",
        '<command-name>/dynamic-smoke</command-name>',
        '<command-args>with args</command-args>',
        '!message.content.contains("<command-name>")',
    ]:
        require(keybinding_smoke, needle, "slash skill submission regression")

    for needle in [
        "render_snapshot_extended_tools_use_semantic_cards_not_raw_json",
        '<command-name>/review</command-name>',
        '<command-args>src</command-args>',
        '"<command-name>"',
    ]:
        require(render_snapshot, needle, "Skill semantic render regression")

    require(
        run_all,
        "wave_w218_skill_invocation_command_tags_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Skill invocation command-tag handoff",
        "phase note",
    )

    print("wave_w218_skill_invocation_command_tags_smoke: ok")


if __name__ == "__main__":
    main()
