#!/usr/bin/env python3
"""
GAP 3: 跨窗口验证 **4 个新 memory 类型** + 1 个负面 case。
AutoMem (第 5 类) 由 cross_window_memory_real_smoke 单独覆盖；本 smoke 不重测。

⚠️ 命名诚实性：之前文件名 cross_window_memory_5types 暗示新 smoke 测了 5 类 —
   实际只测了 4 类新 + 1 个 negative case。叠数误导。本次重命名 4newtypes 反映真覆盖。
   AutoMem 验证在 cross_window_memory_real_smoke.py，两个 smoke 加起来才覆盖 5 类。

契约（4 类新 type 各 1 case + 1 negative = 5 cases）：
  1. 写 Project (cwd/MOSSEN.md) → loader 找到 marker (type=Project)
  2. 写 User (~/.mossen/MOSSEN.md) → loader 找到 marker (type=User)
  3. 写 Local (cwd/MOSSEN.local.md) → loader 找到 marker (type=Local)
  4. 写 ProjectRules (.mossen/rules/test_xxx.md) → loader 找到 marker (type=Project, path 含 rules/)
  5. NEGATIVE: 不写文件 → marker NOT 出现 (防假阳性)

反面案例：
  ❌ 反 1: 写到 raw fs 但 mossen loader 不读这条路径 — 白测
  ❌ 反 2: marker 因 cache 没看到 — 独立 bun process 避免 cache

User path:
  Process A: 写 marker 到 mossen 已知路径
  Process B (独立 bun): 调 getMemoryFiles() — 与 mossen 启动相同代码路径
  断言: 返回的 file 列表中找到 marker

清理: 测试自带 backup-restore + delete pattern; 失败时也保证恢复。
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")
HOME = Path.home()
MOSSEN_USER_MD = HOME / ".mossen" / "MOSSEN.md"
PROJECT_MD = ROOT / "MOSSEN.md"
PROJECT_LOCAL_MD = ROOT / "MOSSEN.local.md"


class MemoryFileBackup:
    """Backup/restore a file (or its absence) for safe smoke testing."""
    def __init__(self, path: Path):
        self.path = path
        self.existed = path.exists()
        self.backup_content = path.read_text(encoding="utf-8") if self.existed else None

    def restore(self):
        if self.existed:
            self.path.write_text(self.backup_content or "", encoding="utf-8")
        elif self.path.exists():
            self.path.unlink()


def _bun_call_loader() -> dict:
    """Run getMemoryFiles in independent bun process. Returns parsed JSON."""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { getMemoryFiles } from './utils/mossenmd.ts';"
        "const files = await getMemoryFiles();"
        "process.stdout.write(JSON.stringify({"
        "  count: files.length,"
        "  entries: files.map((f: any) => ({type: f.type, path: f.path, content: f.content})),"
        "}) + '\\n');"
    )
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=60,
        env=os.environ.copy(),
    )
    if proc.returncode != 0:
        raise RuntimeError(f"loader failed: {proc.stderr[:500]}")
    for line in reversed((proc.stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            return json.loads(line)
    raise RuntimeError(f"loader no json output. stdout={proc.stdout[:300]!r}")


def _has_marker_in_type(loader_result: dict, marker: str, expected_type: str,
                        path_contains: str | None = None) -> tuple[bool, str | None]:
    """Find an entry of expected_type containing marker; optionally path must contain substring.
    Returns (found, matching_path)."""
    for entry in loader_result["entries"]:
        if entry["type"] != expected_type:
            continue
        if marker not in (entry.get("content") or ""):
            continue
        if path_contains and path_contains not in entry["path"]:
            continue
        return True, entry["path"]
    return False, None


def case_project_mossenmd() -> dict:
    """1. Project (cwd MOSSEN.md) marker visible via loader."""
    backup = MemoryFileBackup(PROJECT_MD)
    marker = f"GAP3_PROJECT_MARKER_{int(time.time())}"
    try:
        old = backup.backup_content or ""
        PROJECT_MD.write_text(old + "\n" + marker + "\n", encoding="utf-8")
        loader_result = _bun_call_loader()
        found, path = _has_marker_in_type(loader_result, marker, "Project")
        return {
            "name": "L5.1_project_mossenmd_cross_window",
            "ok": found,
            "marker": marker,
            "expected_path": str(PROJECT_MD),
            "found_at_path": path,
            "loader_count": loader_result["count"],
            "loader_types": list({e["type"] for e in loader_result["entries"]}),
        }
    finally:
        backup.restore()


def case_local_mossenmd() -> dict:
    """3. Local (MOSSEN.local.md) marker visible via loader."""
    backup = MemoryFileBackup(PROJECT_LOCAL_MD)
    marker = f"GAP3_LOCAL_MARKER_{int(time.time())}"
    try:
        old = backup.backup_content or ""
        PROJECT_LOCAL_MD.write_text(old + "\n" + marker + "\n", encoding="utf-8")
        loader_result = _bun_call_loader()
        found, path = _has_marker_in_type(loader_result, marker, "Local")
        return {
            "name": "L5.3_local_mossenmd_cross_window",
            "ok": found,
            "marker": marker,
            "expected_path": str(PROJECT_LOCAL_MD),
            "found_at_path": path,
            "loader_count": loader_result["count"],
            "loader_types": list({e["type"] for e in loader_result["entries"]}),
        }
    finally:
        backup.restore()


def case_user_mossenmd() -> dict:
    """2. User (~/.mossen/MOSSEN.md) marker visible via loader."""
    MOSSEN_USER_MD.parent.mkdir(parents=True, exist_ok=True)
    backup = MemoryFileBackup(MOSSEN_USER_MD)
    marker = f"GAP3_USER_MARKER_{int(time.time())}"
    try:
        old = backup.backup_content or ""
        MOSSEN_USER_MD.write_text(old + "\n" + marker + "\n", encoding="utf-8")
        loader_result = _bun_call_loader()
        found, path = _has_marker_in_type(loader_result, marker, "User")
        return {
            "name": "L5.2_user_mossenmd_cross_window",
            "ok": found,
            "marker": marker,
            "expected_path": str(MOSSEN_USER_MD),
            "found_at_path": path,
            "loader_count": loader_result["count"],
            "loader_types": list({e["type"] for e in loader_result["entries"]}),
        }
    finally:
        backup.restore()


def case_project_rules() -> dict:
    """4. ProjectRules (.mossen/rules/test_<ts>.md) marker via loader (type=Project, path contains rules/)."""
    rules_dir = ROOT / ".mossen" / "rules"
    rules_dir.mkdir(parents=True, exist_ok=True)
    rule_file = rules_dir / f"_gap3_test_{int(time.time())}.md"
    marker = f"GAP3_RULES_MARKER_{int(time.time())}"
    try:
        rule_file.write_text(marker + "\n", encoding="utf-8")
        loader_result = _bun_call_loader()
        # Rules are returned as type='Project' but path contains 'rules/'
        found, path = _has_marker_in_type(
            loader_result, marker, "Project", path_contains=".mossen/rules"
        )
        return {
            "name": "L5.4_project_rules_cross_window",
            "ok": found,
            "marker": marker,
            "expected_path": str(rule_file),
            "found_at_path": path,
            "loader_count": loader_result["count"],
            "loader_types": list({e["type"] for e in loader_result["entries"]}),
        }
    finally:
        if rule_file.exists():
            rule_file.unlink()
        # Cleanup empty .mossen/rules dir if we created it
        try:
            if rules_dir.exists() and not list(rules_dir.iterdir()):
                rules_dir.rmdir()
        except OSError:
            pass


def case_negative_marker_not_in_loader_when_no_file() -> dict:
    """Negative: marker not in any source file → loader doesn't return it.
    防假阳性：确保我们的 marker 检测真依赖文件存在。"""
    marker = f"GAP3_NEGATIVE_MARKER_{int(time.time())}"
    # Don't write anywhere
    loader_result = _bun_call_loader()
    found_anywhere = any(
        marker in (e.get("content") or "") for e in loader_result["entries"]
    )
    return {
        "name": "L5_negative_marker_not_found_when_no_file",
        "ok": not found_anywhere,
        "marker": marker,
        "loader_count": loader_result["count"],
    }


def main() -> int:
    results = [
        case_negative_marker_not_in_loader_when_no_file(),  # baseline防假阳性
        case_project_mossenmd(),
        case_local_mossenmd(),
        case_user_mossenmd(),
        case_project_rules(),
    ]
    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
