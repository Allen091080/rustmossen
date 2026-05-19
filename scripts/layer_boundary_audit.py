#!/usr/bin/env python3
"""Audit high-risk imports across Core / CLI / Workbench boundaries."""

from __future__ import annotations

import fnmatch
import json
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parents[1]
RULES_PATH = ROOT / "scripts" / "layer-boundary-rules.json"

IMPORT_RE = re.compile(
    r"""^\s*(?:import|export)\s+(?:type\s+)?(?:[\s\S]*?\s+from\s+)?["']([^"']+)["']""",
    re.MULTILINE,
)
DYNAMIC_RE = re.compile(r"""(?:import|require)\(\s*["']([^"']+)["']\s*\)""")


@dataclass(frozen=True)
class ImportRef:
    source: str
    line: int
    specifier: str
    targets: tuple[str, ...]


def as_posix(path: Path) -> str:
    return path.as_posix()


def is_ignored(path: Path, ignore_dirs: Iterable[str]) -> bool:
    parts = set(path.parts)
    return any(item in parts for item in ignore_dirs)


def iter_source_files(config: dict) -> Iterable[Path]:
    extensions = set(config["sourceExtensions"])
    ignore_dirs = set(config["ignoreDirs"])
    for dirpath, dirnames, filenames in os.walk(ROOT):
        dirnames[:] = [name for name in dirnames if name not in ignore_dirs]
        for filename in filenames:
            path = Path(dirpath) / filename
            if path.suffix not in extensions:
                continue
            rel = path.relative_to(ROOT)
            if is_ignored(rel, ignore_dirs):
                continue
            yield path


def line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def strip_known_extension(path: str) -> str:
    for suffix in (".js", ".jsx", ".ts", ".tsx"):
        if path.endswith(suffix):
            return path[: -len(suffix)]
    return path


def normalize_specifier(source: Path, specifier: str) -> tuple[str, ...]:
    if specifier.startswith("."):
        base = (source.parent / specifier).resolve()
        try:
            rel = as_posix(base.relative_to(ROOT))
        except ValueError:
            return ()
    elif specifier.startswith("src/"):
        rel = specifier[len("src/") :]
    else:
        return ()

    rel = rel.lstrip("/")
    stem = strip_known_extension(rel)
    candidates = {rel, stem}
    for suffix in (".ts", ".tsx", ".js", ".jsx", ".json"):
        candidates.add(f"{stem}{suffix}")
    for index in ("index.ts", "index.tsx", "index.js", "index.jsx"):
        candidates.add(f"{stem}/{index}")
    return tuple(sorted(candidates))


def iter_imports(path: Path) -> Iterable[ImportRef]:
    text = path.read_text(encoding="utf-8", errors="ignore")
    rel = as_posix(path.relative_to(ROOT))
    seen: set[tuple[int, str]] = set()
    for regex in (IMPORT_RE, DYNAMIC_RE):
        for match in regex.finditer(text):
            specifier = match.group(1)
            key = (match.start(1), specifier)
            if key in seen:
                continue
            seen.add(key)
            targets = normalize_specifier(path, specifier)
            if not targets:
                continue
            yield ImportRef(rel, line_number(text, match.start(1)), specifier, targets)


def matches_any(path: str, patterns: Iterable[str]) -> bool:
    return any(fnmatch.fnmatch(path, pattern) for pattern in patterns)


def allowlisted(config: dict, rule_id: str, source: str, target: str) -> bool:
    for item in config.get("allowlist", []):
        if item.get("rule") != rule_id:
            continue
        if item.get("source") == source and item.get("target") == target:
            return True
    return False


def main() -> int:
    config = json.loads(RULES_PATH.read_text(encoding="utf-8"))
    files = list(iter_source_files(config))
    imports = [ref for path in files for ref in iter_imports(path)]
    violations: list[tuple[dict, ImportRef, str]] = []

    for ref in imports:
        for rule in config["rules"]:
            if not matches_any(ref.source, rule["from"]):
                continue
            for target in ref.targets:
                if matches_any(target, rule["to"]) and not allowlisted(
                    config, rule["id"], ref.source, target
                ):
                    violations.append((rule, ref, target))

    if violations:
        print("FAIL: layer boundary audit found violations")
        for rule, ref, target in violations:
            print(f"  {ref.source}:{ref.line} imports {target}")
            print(f"    specifier: {ref.specifier}")
            print(f"    rule: {rule['id']}")
            print(f"    reason: {rule['reason']}")
        return 1

    print("PASS: layer boundary audit")
    print(f"  scanned files : {len(files)}")
    print(f"  imports seen  : {len(imports)}")
    print(f"  rules         : {len(config['rules'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
