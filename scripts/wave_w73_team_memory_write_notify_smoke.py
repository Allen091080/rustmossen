#!/usr/bin/env python3
"""W73 - team memory write notification smoke.

Guards the production write/edit tool path that should wake the team-memory
watcher after a successful write to the resolved team-memory directory.
"""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
SERVICE = ROOT / "crates/mossen-agent/src/services/team_memory_sync/service.rs"
MOD = ROOT / "crates/mossen-agent/src/services/team_memory_sync/mod.rs"
FILE_WRITE = ROOT / "crates/mossen-tools/src/file_write.rs"
FILE_EDIT = ROOT / "crates/mossen-tools/src/file_edit.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def require_contains(failures: list[str], path: Path, needle: str, message: str) -> None:
    text = path.read_text()
    if needle not in text:
        fail(failures, message)


def main() -> int:
    failures: list[str] = []
    service = SERVICE.read_text()
    mod = MOD.read_text()
    file_write = FILE_WRITE.read_text()
    file_edit = FILE_EDIT.read_text()
    run_all = RUN_ALL.read_text()

    require_contains(
        failures,
        SERVICE,
        "pub fn is_team_memory_file_path(file_path: impl AsRef<Path>) -> bool",
        "service.rs must expose team-memory path detection",
    )
    if "get_team_memory_dir()" not in service or "starts_with(existing_or_absolute_path(dir))" not in service:
        fail(
            failures,
            "team-memory path detection must compare against the resolved team-memory dir",
        )

    require_contains(
        failures,
        MOD,
        "pub async fn notify_team_memory_file_write(file_path: impl AsRef<Path>)",
        "team_memory_sync mod must expose path-gated write notification",
    )
    if (
        "service::is_team_memory_file_path(file_path.as_ref())" not in mod
        or "watcher::notify_team_memory_write().await" not in mod
    ):
        fail(
            failures,
            "notify_team_memory_file_write must gate by path and notify the watcher",
        )

    if "tmp.persist(path)?" not in file_write:
        fail(failures, "FileComposer must still use the atomic persist write path")
    if "notify_team_memory_file_write(&full_path).await" not in file_write:
        fail(failures, "FileComposer must notify team-memory writes after persist")

    edit_notify_count = file_edit.count("notify_team_memory_file_write")
    if edit_notify_count < 3:
        fail(
            failures,
            "SourcePatcher must notify team-memory writes for create, empty-file write, and edit",
        )
    update_write_idx = file_edit.find("atomic_write(&full_path, &updated).await?")
    update_notify_idx = file_edit.find(
        "notify_team_memory_file_write(&full_path).await", update_write_idx
    )
    if update_write_idx == -1 or update_notify_idx == -1 or update_write_idx > update_notify_idx:
        fail(failures, "SourcePatcher update notification must occur after atomic_write")

    if "wave_w73_team_memory_write_notify_smoke" not in run_all:
        fail(failures, "run_all_smoke.sh must register W73")

    print("=== W73 team memory write notify smoke ===")
    print(f"service: {SERVICE.relative_to(ROOT)}")
    print(f"write tool: {FILE_WRITE.relative_to(ROOT)}")
    print(f"edit tool: {FILE_EDIT.relative_to(ROOT)}")
    if failures:
        print("\nFAILURES:")
        for failure in failures:
            print(f" - {failure}")
        return 1
    print(
        "PASS: Write/Edit tools path-gate successful team-memory writes and notify the watcher"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
