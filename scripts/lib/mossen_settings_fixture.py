"""
mossen_settings_fixture — 共享 settings.json 读写 helper, 给 R5/R6/R8 用.

G2-2b 抽出 (G0-5 测试矩阵设计建议).

API:
  write_user_settings(home_dir, key, value)
  write_project_settings(proj_dir, key, value)
  read_user_settings(home_dir) -> dict
  read_project_settings(proj_dir) -> dict
  clear_all_overrides(home_dir, proj_dir)

home_dir = MOSSEN_CONFIG_DIR (fixture 隔离); 不传时用 ~/.mossen.
proj_dir = <project root>/.mossen.
"""

from __future__ import annotations

import json
from pathlib import Path


def _user_settings_path(home_dir: Path) -> Path:
    return Path(home_dir) / "settings.json"


def _project_settings_path(proj_dir: Path) -> Path:
    return Path(proj_dir) / ".mossen" / "settings.json"


def write_user_settings(home_dir: Path, key: str, value) -> None:
    p = _user_settings_path(home_dir)
    p.parent.mkdir(parents=True, exist_ok=True)
    current = read_user_settings(home_dir) or {}
    current[key] = value
    p.write_text(json.dumps(current, indent=2) + "\n")


def write_project_settings(proj_dir: Path, key: str, value) -> None:
    p = _project_settings_path(proj_dir)
    p.parent.mkdir(parents=True, exist_ok=True)
    current = read_project_settings(proj_dir) or {}
    current[key] = value
    p.write_text(json.dumps(current, indent=2) + "\n")


def read_user_settings(home_dir: Path) -> dict | None:
    p = _user_settings_path(home_dir)
    if not p.exists():
        return None
    try:
        return json.loads(p.read_text())
    except (json.JSONDecodeError, OSError):
        return None


def read_project_settings(proj_dir: Path) -> dict | None:
    p = _project_settings_path(proj_dir)
    if not p.exists():
        return None
    try:
        return json.loads(p.read_text())
    except (json.JSONDecodeError, OSError):
        return None


def clear_all_overrides(home_dir: Path, proj_dir: Path) -> None:
    """清掉 user + project settings.json. 不存在的文件 = no-op."""
    for p in (_user_settings_path(home_dir), _project_settings_path(proj_dir)):
        if p.exists():
            try:
                p.unlink()
            except OSError:
                pass
