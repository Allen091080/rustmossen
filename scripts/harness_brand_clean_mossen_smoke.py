#!/usr/bin/env python3
"""Repository-wide Mossen brand hygiene smoke."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SKIP_DIRS = {
    ".git",
    ".mossen",
    ".mossensrc",
    "node_modules",
    "target",
    "dist",
    "build",
    "coverage",
}
SKIP_FILES = {"Cargo.lock"}
TEXT_EXTS = {
    ".rs",
    ".py",
    ".md",
    ".toml",
    ".json",
    ".sh",
    ".yml",
    ".yaml",
    ".txt",
}


def token(*codes: int) -> str:
    return "".join(chr(code) for code in codes)


SUBSTRING_TOKENS = [
    token(97, 110, 116, 104, 114, 111, 112, 105, 99),
    token(99, 108, 97, 117, 100, 101),
    token(99, 108, 117, 97, 100, 101),
    token(115, 111, 110, 110, 101, 116),
    token(104, 97, 105, 107, 117),
    token(111, 112, 117, 115),
    token(102, 114, 111, 110, 116, 105, 101, 114),
    token(116, 101, 110, 103, 117),
    token(102, 101, 110, 110, 101, 99),
]
WORD_TOKENS = [token(97, 110, 116)]


def iter_text_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*"):
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if not path.is_file() or path.name in SKIP_FILES:
            continue
        if path.suffix.lower() in TEXT_EXTS or path.name in {"README", "AGENTS"}:
            files.append(path)
    return files


def path_word_hits(path: Path, word: str) -> bool:
    rel = path.relative_to(ROOT).as_posix().lower()
    return word in re.split(r"[^a-z0-9]+", rel)


def main() -> int:
    hits: list[dict[str, object]] = []
    for path in iter_text_files():
        rel = path.relative_to(ROOT).as_posix()
        lowered_rel = rel.lower()
        for banned in SUBSTRING_TOKENS:
            if banned in lowered_rel:
                hits.append({"where": "path", "path": rel, "token": banned})
        for banned in WORD_TOKENS:
            if path_word_hits(path, banned):
                hits.append({"where": "path", "path": rel, "token": banned})

        try:
            text = path.read_text(errors="ignore")
        except OSError:
            continue
        lowered = text.lower()
        for banned in SUBSTRING_TOKENS:
            if banned not in lowered:
                continue
            for lineno, line in enumerate(text.splitlines(), 1):
                if banned in line.lower():
                    hits.append(
                        {
                            "where": "content",
                            "path": rel,
                            "line": lineno,
                            "token": banned,
                            "snippet": line.strip()[:180],
                        }
                    )
        for banned in WORD_TOKENS:
            pattern = re.compile(rf"\b{re.escape(banned)}\b", re.IGNORECASE)
            for lineno, line in enumerate(text.splitlines(), 1):
                if pattern.search(line):
                    hits.append(
                        {
                            "where": "content",
                            "path": rel,
                            "line": lineno,
                            "token": banned,
                            "snippet": line.strip()[:180],
                        }
                    )

    result = {
        "ok": not hits,
        "checked_files": len(iter_text_files()),
        "hits": hits[:200],
        "truncated": max(0, len(hits) - 200),
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
