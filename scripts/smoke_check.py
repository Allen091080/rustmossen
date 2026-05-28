#!/usr/bin/env python3
"""Current repository smoke gate.

This intentionally stays small: it verifies that the active Rust terminal UI
uses the three-layer render path and that retired compatibility assets do not
creep back in. Banned tokens are assembled from fragments so the scanner does
not report this guard as its own failure.
"""

from __future__ import annotations

import argparse
import os
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


def split_token(*parts: str) -> str:
    return "".join(parts)


def codes_token(*codes: int) -> str:
    return "".join(chr(code) for code in codes)


RETIRED_RENDER_TOKENS = [
    split_token("i", "nk"),
    split_token("i", "nk", "_", "utils"),
    split_token("to", "_", "i", "nk", "_", "color"),
    split_token("react", "-", "i", "nk"),
    split_token("Message", "Widget"),
    split_token("message", "_", "row"),
    split_token("root", "_", "small"),
    split_token("TS", "/", "I", "nk"),
    split_token("i", "nk", "-", "translated"),
]
RETIRED_BRAND_SUBSTRING_TOKENS = [
    codes_token(97, 110, 116, 104, 114, 111, 112, 105, 99),
    codes_token(99, 108, 97, 117, 100, 101),
    codes_token(99, 108, 117, 97, 100, 101),
    codes_token(115, 111, 110, 110, 101, 116),
    codes_token(104, 97, 105, 107, 117),
    codes_token(111, 112, 117, 115),
    codes_token(102, 114, 111, 110, 116, 105, 101, 114),
    codes_token(116, 101, 110, 103, 117),
    codes_token(102, 101, 110, 110, 101, 99),
]
RETIRED_BRAND_WORD_TOKENS = [
    codes_token(97, 110, 116),
]
REQUIRED_RENDER_FILES = [
    "crates/mossen-tui/src/render_model.rs",
    "crates/mossen-tui/src/render_profile.rs",
    "crates/mossen-tui/src/render_lifecycle.rs",
    "crates/mossen-tui/src/render_cache.rs",
    "crates/mossen-tui/src/widgets/render_block.rs",
    "crates/mossen-tui/src/widgets/approval.rs",
]


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


def line_hits(path: Path, token: str, *, word_boundary: bool) -> list[tuple[int, str]]:
    try:
        text = path.read_text(errors="ignore")
    except OSError:
        return []
    flags = re.IGNORECASE
    if word_boundary:
        pattern = re.compile(rf"\b{re.escape(token)}\b", flags)
    else:
        pattern = re.compile(re.escape(token), flags)
    hits: list[tuple[int, str]] = []
    for lineno, line in enumerate(text.splitlines(), 1):
        if pattern.search(line):
            hits.append((lineno, line.strip()[:180]))
    return hits


def check_tokens(name: str, tokens: list[str], *, word_boundary: bool) -> int:
    failures = 0
    for token in tokens:
        for path in iter_text_files():
            hits = line_hits(path, token, word_boundary=word_boundary)
            if not hits:
                continue
            rel = path.relative_to(ROOT)
            for lineno, snippet in hits[:8]:
                print(f"[{name}] {rel}:{lineno}: {snippet}")
                failures += 1
            if len(hits) > 8:
                print(f"[{name}] {rel}: ... {len(hits) - 8} more")
    return failures


def check_render_files() -> int:
    failures = 0
    for rel in REQUIRED_RENDER_FILES:
        if not (ROOT / rel).exists():
            print(f"[render-pipeline] missing {rel}")
            failures += 1
    retired_dirs = [
        Path("crates/mossen-tui/src") / split_token("i", "nk"),
        Path("crates/mossen-utils/src") / f"{split_token('i', 'nk')}.rs",
        Path("crates/mossen-utils/src") / f"{split_token('i', 'nk')}_utils.rs",
    ]
    for rel in retired_dirs:
        if (ROOT / rel).exists():
            print(f"[render-pipeline] retired path still exists: {rel}")
            failures += 1
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--only", default="", help="Comma-separated check names.")
    args = parser.parse_args()
    selected = {name.strip() for name in args.only.split(",") if name.strip()}

    checks = {
        "retired_render_tokens": lambda: check_tokens(
            "retired-render", RETIRED_RENDER_TOKENS, word_boundary=True
        ),
        "retired_brand_tokens": lambda: check_tokens(
            "retired-brand", RETIRED_BRAND_SUBSTRING_TOKENS, word_boundary=False
        )
        + check_tokens(
            "retired-brand-word", RETIRED_BRAND_WORD_TOKENS, word_boundary=True
        ),
        "render_pipeline_files": check_render_files,
    }
    if selected:
        unknown = selected - set(checks)
        if unknown:
            print(
                "Retired check names mapped to the current compact gate: "
                + ", ".join(sorted(unknown))
            )
            selected = set(checks)
        check_items = [(name, checks[name]) for name in sorted(selected)]
    else:
        check_items = list(checks.items())

    failures = 0
    for name, check in check_items:
        check_failures = check()
        failures += check_failures
        if check_failures:
            print(f"{name}: failed")
        else:
            print(f"{name}: ok")
    return 1 if failures else 0


if __name__ == "__main__":
    os.chdir(ROOT)
    raise SystemExit(main())
