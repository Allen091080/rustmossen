#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    dialogue = read("crates/mossen-agent/src/dialogue.rs")
    structured = read("crates/mossen-cli/src/structured_io.rs")
    run_all = read("scripts/run_all_smoke.sh")
    phase = read("phases/03g-rendering-product-grade-plan.md")

    for needle in [
        'const PERMISSION_ALLOW_RULES_ENV: &str = "MOSSEN_PERMISSION_ALLOW_RULES"',
        'const PERMISSION_DENY_RULES_ENV: &str = "MOSSEN_PERMISSION_DENY_RULES"',
        "fn session_permission_rule_decision",
        "permission_rule_env_lines(PERMISSION_DENY_RULES_ENV)",
        "permission_rule_env_lines(PERMISSION_ALLOW_RULES_ENV)",
        "fn permission_rules_match",
        "fn permission_rule_candidates",
        "fn wildcard_permission_rule_matches",
        "fn permission_rule_path_prefix_matches",
        "let rule_decision = if needs_permission",
        "needs_permission && rule_decision.is_none()",
        '"session_permission_rules"',
        "Tool call denied by session permission rule.",
        "session_permission_rules_allow_matching_tool_inputs",
        "session_permission_rules_deny_precedes_allow",
        "session_permission_rules_match_file_path_prefixes",
    ]:
        require(dialogue, needle, "agent session permission rule enforcement")

    for needle in [
        "PERMISSION_ALLOW_RULES_ENV",
        "PERMISSION_DENY_RULES_ENV",
        "apply_session_permission_rule",
        '"rule_update"',
    ]:
        require(structured, needle, "stream-json rule command bridge")

    require(
        run_all,
        "wave_w224_agent_session_permission_rules_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Agent session permission rules",
        "phase note",
    )

    print("wave_w224_agent_session_permission_rules_smoke: ok")


if __name__ == "__main__":
    main()
