#!/usr/bin/env python3
"""W74 - team-memory path alignment smoke.

Guards that the sync/watcher default directory matches the team-memory path
that the CLI memory prompt exposes to the model.
"""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
MEMDIR = ROOT / "crates/mossen-cli/src/memdir.rs"
SERVICE = ROOT / "crates/mossen-agent/src/services/team_memory_sync/service.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def main() -> int:
    failures: list[str] = []
    memdir = MEMDIR.read_text()
    service = SERVICE.read_text()
    run_all = RUN_ALL.read_text()

    if 'const AUTO_MEM_DIRNAME: &str = "memory";' not in memdir:
        fail(failures, "CLI auto-memory dirname must stay `memory`")
    if 'get_auto_mem_path(project_root).join("team")' not in memdir:
        fail(failures, "CLI team-memory path must remain auto-memory root plus /team")

    required_service_snippets = [
        "fn resolve_project_team_memory_dir(",
        'get_env_config_value(&["MOSSEN_COWORK_MEMORY_PATH_OVERRIDE"])',
        'get_env_config_value(&["MOSSEN_CODE_REMOTE_MEMORY_DIR"])',
        "mossen_utils::env::get_mossen_config_home_dir()",
        '.join("projects")',
        '.join("memory")',
        '.join("team")',
    ]
    for snippet in required_service_snippets:
        if snippet not in service:
            fail(failures, f"service.rs missing path-alignment snippet: {snippet}")

    if 'unwrap_or_else(|| PathBuf::from(".mossen/team-memory"))' in service:
        fail(
            failures,
            "service.rs must not default to the old project-local .mossen/team-memory path",
        )

    if "resolve_project_team_memory_dir_matches_cli_prompt_path_shape" not in service:
        fail(failures, "service.rs must unit-test CLI prompt path alignment")
    if "resolve_project_team_memory_dir_honors_auto_memory_override_root" not in service:
        fail(failures, "service.rs must unit-test memory override path alignment")

    if "wave_w74_team_memory_path_alignment_smoke" not in run_all:
        fail(failures, "run_all_smoke.sh must register W74")

    print("=== W74 team memory path alignment smoke ===")
    print(f"memdir: {MEMDIR.relative_to(ROOT)}")
    print(f"service: {SERVICE.relative_to(ROOT)}")
    if failures:
        print("\nFAILURES:")
        for failure in failures:
            print(f" - {failure}")
        return 1
    print("PASS: team-memory sync/watcher path aligns with CLI memory prompt path")
    return 0


if __name__ == "__main__":
    sys.exit(main())
