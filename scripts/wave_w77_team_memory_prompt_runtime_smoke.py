#!/usr/bin/env python3
"""W77 - team-memory prompt/runtime wiring smoke.

Guards the user-visible memory layer: when team memory is available, the
system prompt and runtime snapshot must expose the same team-memory path that
sync/watch/write tooling uses.
"""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
MEMDIR = ROOT / "crates/mossen-cli/src/memdir.rs"
SYSTEM_PROMPT = ROOT / "crates/mossen-cli/src/system_prompt.rs"
INTERACTIVE = ROOT / "crates/mossen-cli/src/interactive.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def main() -> int:
    failures: list[str] = []
    memdir = MEMDIR.read_text()
    system_prompt = SYSTEM_PROMPT.read_text()
    interactive = INTERACTIVE.read_text()
    run_all = RUN_ALL.read_text()

    if "pub fn is_team_memory_rollout_enabled()" not in memdir:
        fail(failures, "memdir.rs must expose team-memory rollout availability")
    for snippet in [
        '"MOSSEN_CODE_ENABLE_TEAM_MEMORY"',
        '"MOSSEN_TEAM_MEMORY"',
        "team_memory_sync::is_team_memory_sync_available()",
        "is_auto_memory_enabled() && is_team_memory_rollout_enabled()",
    ]:
        if snippet not in memdir:
            fail(failures, f"memdir.rs missing gate snippet: {snippet}")
    enabled_start = memdir.find("pub fn is_team_memory_enabled() -> bool")
    enabled_end = memdir.find("pub fn is_team_memory_rollout_enabled", enabled_start)
    enabled_body = memdir[enabled_start:enabled_end]
    if "Feature-gated: default false" in enabled_body or "false" in enabled_body:
        fail(failures, "memdir.rs must not keep team memory hardcoded false")
    if "ensure_memory_dir_exists(&auto_dir).await;" not in memdir:
        fail(failures, "combined team-memory prompt must ensure the private auto dir")
    if "ensure_memory_dir_exists(&team_dir).await;" not in memdir:
        fail(failures, "combined team-memory prompt must ensure the team dir")
    if "team_memory_rollout_uses_explicit_flags_before_sync_availability" not in memdir:
        fail(failures, "memdir.rs must unit-test team-memory rollout precedence")
    if "team_memory_path_detection_uses_component_boundaries" not in memdir:
        fail(failures, "memdir.rs must unit-test team-memory path boundaries")

    if "crate::memdir::load_memory_prompt(cwd).await" not in system_prompt:
        fail(failures, "system_prompt.rs must inject memdir memory prompt text")

    for snippet in [
        "let auto_memory_enabled = crate::memdir::is_auto_memory_enabled();",
        "let rollout_enabled = crate::memdir::is_team_memory_rollout_enabled();",
        "let enabled = crate::memdir::is_team_memory_enabled();",
        "team_memory_sync::is_team_memory_sync_available()",
        "crate::memdir::get_team_mem_path(&project_root)",
        "crate::memdir::get_team_mem_entrypoint(&project_root)",
        "build_enabled: true",
    ]:
        if snippet not in interactive:
            fail(failures, f"interactive.rs missing runtime snapshot snippet: {snippet}")
    if "build_enabled: false" in interactive:
        fail(failures, "team-memory runtime snapshot must not be hardcoded unavailable")

    if "wave_w77_team_memory_prompt_runtime_smoke" not in run_all:
        fail(failures, "run_all_smoke.sh must register W77")

    print("=== W77 team memory prompt/runtime smoke ===")
    print(f"memdir: {MEMDIR.relative_to(ROOT)}")
    print(f"system prompt: {SYSTEM_PROMPT.relative_to(ROOT)}")
    print(f"runtime snapshot: {INTERACTIVE.relative_to(ROOT)}")
    if failures:
        print("\nFAILURES:")
        for failure in failures:
            print(f" - {failure}")
        return 1
    print("PASS: team-memory prompt and runtime snapshot are wired to real availability")
    return 0


if __name__ == "__main__":
    sys.exit(main())
