#!/usr/bin/env python3
"""W75 - team-memory secret guard smoke.

Guards the production Write/Edit path that prevents detected secrets from
being written into the resolved team-memory directory.
"""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
SECRET_GUARD = ROOT / "crates/mossen-agent/src/services/team_memory_sync/secret_guard.rs"
FILE_WRITE = ROOT / "crates/mossen-tools/src/file_write.rs"
FILE_EDIT = ROOT / "crates/mossen-tools/src/file_edit.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def main() -> int:
    failures: list[str] = []
    secret_guard = SECRET_GUARD.read_text()
    file_write = FILE_WRITE.read_text()
    file_edit = FILE_EDIT.read_text()
    run_all = RUN_ALL.read_text()

    if "service::is_team_memory_file_path(path)" not in secret_guard:
        fail(
            failures,
            "secret_guard.rs must use the resolved team-memory path detector",
        )
    if 's == ".mossen"' in secret_guard or 's == "team-memory"' in secret_guard:
        fail(failures, "secret_guard.rs must not use the old marker heuristic")
    if "secret_guard_blocks_detected_secret_for_team_memory_path" not in secret_guard:
        fail(failures, "secret_guard.rs must unit-test secret blocking")

    write_guard_idx = file_write.find("check_team_mem_secrets(path, &inp.content)")
    write_persist_idx = file_write.find("tmp.persist(path)?")
    if write_guard_idx == -1:
        fail(failures, "FileComposer must call check_team_mem_secrets")
    if write_persist_idx == -1:
        fail(failures, "FileComposer must keep the atomic persist write path")
    if (
        write_guard_idx == -1
        or write_persist_idx == -1
        or write_guard_idx > write_persist_idx
    ):
        fail(failures, "FileComposer must reject team-memory secrets before persist")

    if "fn team_memory_secret_error(" not in file_edit:
        fail(failures, "SourcePatcher must have a shared team-memory secret guard")
    edit_guard_count = file_edit.count("team_memory_secret_error(&full_path")
    if edit_guard_count < 3:
        fail(
            failures,
            "SourcePatcher must guard create, empty-file write, and update branches",
        )
    update_idx = file_edit.find("let updated = if inp.replace_all")
    update_guard_idx = file_edit.find(
        "team_memory_secret_error(&full_path, &updated)", update_idx
    )
    update_write_idx = file_edit.find("atomic_write(&full_path, &updated).await?", update_idx)
    if (
        update_idx == -1
        or update_guard_idx == -1
        or update_write_idx == -1
        or update_guard_idx > update_write_idx
    ):
        fail(failures, "SourcePatcher must reject updated secrets before atomic_write")

    if "wave_w75_team_memory_secret_guard_smoke" not in run_all:
        fail(failures, "run_all_smoke.sh must register W75")

    print("=== W75 team memory secret guard smoke ===")
    print(f"secret guard: {SECRET_GUARD.relative_to(ROOT)}")
    print(f"write tool: {FILE_WRITE.relative_to(ROOT)}")
    print(f"edit tool: {FILE_EDIT.relative_to(ROOT)}")
    if failures:
        print("\nFAILURES:")
        for failure in failures:
            print(f" - {failure}")
        return 1
    print("PASS: Write/Edit reject detected secrets before team-memory writes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
