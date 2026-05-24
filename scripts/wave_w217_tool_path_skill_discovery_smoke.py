#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    dynamic = read("crates/mossen-skills/src/dynamic.rs")
    skill_discovery = read("crates/mossen-tools/src/skill_discovery.rs")
    lib_rs = read("crates/mossen-tools/src/lib.rs")
    file_read = read("crates/mossen-tools/src/file_read.rs")
    file_write = read("crates/mossen-tools/src/file_write.rs")
    file_edit = read("crates/mossen-tools/src/file_edit.rs")
    run_all = read("scripts/run_all_smoke.sh")
    phase = read("phases/03g-rendering-product-grade-plan.md")

    for needle in [
        "let mut current_dir = discovery_start_dir(file_path, &resolved_cwd);",
        "if !is_at_or_under(&dir, &resolved_cwd)",
        "fn discovery_start_dir",
        "fn is_at_or_under",
        "discover_skill_dirs_checks_cwd_level_skills",
    ]:
        require(dynamic, needle, "cwd-level dynamic skill discovery")

    for needle in [
        "pub struct ToolSkillDiscoveryReport",
        "pub async fn observe_tool_file_paths",
        "MAX_OBSERVED_TOOL_PATHS: usize = 32",
        "CANONICAL_CONFIG_DIR: &str = \".mossen\"",
        "\"rawPathsIncluded\": false",
        "\"pathsRedacted\": true",
        "discover_skill_dirs_for_paths(&observed, &cwd_path, CANONICAL_CONFIG_DIR)",
        "add_skill_directories(&discovered).await",
        "activate_conditional_skills_for_paths(&observed_strings, &cwd_path)",
        "observe_tool_file_paths_discovers_project_skill_dir",
    ]:
        require(skill_discovery, needle, "tool path skill discovery hook")

    require(lib_rs, "pub mod skill_discovery;", "mossen-tools module export")

    for source, label in [
        (file_read, "file read hook"),
        (file_write, "file write hook"),
        (file_edit, "file edit hook"),
    ]:
        require(source, "observe_tool_file_paths", label)
        require(source, "&context.cwd", label)
        require(source, "to_metadata()", label)

    require(
        run_all,
        "wave_w217_tool_path_skill_discovery_smoke",
        "run_all registration",
    )
    require(phase, "2026-05-23 Tool-path skill discovery hook", "phase note")

    print("wave_w217_tool_path_skill_discovery_smoke: ok")


if __name__ == "__main__":
    main()
