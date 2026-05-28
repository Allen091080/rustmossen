#!/usr/bin/env python3
"""W76 - team-memory detector alignment smoke.

Guards the non-sync detector layer so analytics/collapse/helper code follows
the current `memory/team` path shape and uses path component boundaries.
"""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
MEMORY_DETECTION = ROOT / "crates/mossen-utils/src/memory_file_detection.rs"
TEAM_MEMORY_OPS = ROOT / "crates/mossen-utils/src/team_memory_ops.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def main() -> int:
    failures: list[str] = []
    memory_detection = MEMORY_DETECTION.read_text()
    team_memory_ops = TEAM_MEMORY_OPS.read_text()
    run_all = RUN_ALL.read_text()

    if "fn path_starts_with_dir(" not in memory_detection:
        fail(failures, "memory_file_detection.rs must use component-boundary path checks")
    stale_prefix_checks = [
        "return normalized.starts_with(&auto_mem_cmp);",
        "return normalized.starts_with(&team_cmp);",
        "normalized_cmp.starts_with(&team_cmp)",
        "let under_config = normalized_cmp.starts_with(&config_dir_cmp)",
        "let under_memory_base = normalized_cmp.starts_with(&memory_base_cmp)",
    ]
    for snippet in stale_prefix_checks:
        if snippet in memory_detection:
            fail(failures, f"memory_file_detection.rs still has stale prefix check: {snippet}")
    for test_name in [
        "memory_path_detection_uses_component_boundaries",
        "session_config_dir_detection_uses_component_boundaries",
        "memory_directory_detection_uses_component_boundaries",
    ]:
        if test_name not in memory_detection:
            fail(failures, f"memory_file_detection.rs missing test: {test_name}")

    if 'const FILE_EDIT_TOOL_NAME: &str = "Edit";' not in team_memory_ops:
        fail(failures, "team_memory_ops.rs must use production Edit tool name")
    if 'const FILE_WRITE_TOOL_NAME: &str = "Write";' not in team_memory_ops:
        fail(failures, "team_memory_ops.rs must use production Write tool name")
    if 'has_component_pair(path, "memory", "team")' not in team_memory_ops:
        fail(failures, "team_memory_ops.rs must detect current memory/team path shape")
    if "is_team_mem_file_matches_current_and_legacy_path_shapes" not in team_memory_ops:
        fail(failures, "team_memory_ops.rs must unit-test current team-memory path shape")
    if "write_or_edit_detection_uses_production_tool_names" not in team_memory_ops:
        fail(failures, "team_memory_ops.rs must unit-test production tool names")

    if "wave_w76_team_memory_detector_alignment_smoke" not in run_all:
        fail(failures, "run_all_smoke.sh must register W76")

    print("=== W76 team memory detector alignment smoke ===")
    print(f"memory detection: {MEMORY_DETECTION.relative_to(ROOT)}")
    print(f"team memory ops: {TEAM_MEMORY_OPS.relative_to(ROOT)}")
    if failures:
        print("\nFAILURES:")
        for failure in failures:
            print(f" - {failure}")
        return 1
    print("PASS: utility team-memory detectors align with memory/team paths")
    return 0


if __name__ == "__main__":
    sys.exit(main())
