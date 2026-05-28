#!/usr/bin/env python3
"""Audit high-risk imports across the current Rust TUI boundaries."""

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
RUST_USE_RE = re.compile(r"""^\s*(?:pub\s+)?use\s+([^;]+);""", re.MULTILINE)


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
    for suffix in (".rs", ".js", ".jsx", ".ts", ".tsx"):
        if path.endswith(suffix):
            return path[: -len(suffix)]
    return path


def crate_src_root(source: Path) -> Path | None:
    rel = source.relative_to(ROOT)
    parts = rel.parts
    if len(parts) >= 3 and parts[0] == "crates" and parts[2] == "src":
        return ROOT / parts[0] / parts[1] / parts[2]
    if "src" in parts:
        index = parts.index("src")
        return ROOT.joinpath(*parts[: index + 1])
    return None


def current_rust_module_parts(source: Path, src_root: Path) -> list[str]:
    rel = source.relative_to(src_root)
    if rel.name == "mod.rs":
        return list(rel.parent.parts)
    return list(rel.with_suffix("").parts)


def rust_module_file(src_root: Path, parts: list[str]) -> Path | None:
    if not parts:
        return None
    direct = src_root.joinpath(*parts).with_suffix(".rs")
    if direct.exists():
        return direct
    module = src_root.joinpath(*parts) / "mod.rs"
    if module.exists():
        return module
    return None


def normalize_rust_specifier(source: Path, specifier: str) -> tuple[str, ...]:
    src_root = crate_src_root(source)
    if src_root is None:
        return ()

    spec = re.sub(r"\s+", "", specifier)
    if "as" in spec:
        spec = spec.split("as", 1)[0]

    base_parts: list[str]
    if spec.startswith("crate::"):
        rest = spec[len("crate::") :]
        base_parts = []
    elif spec.startswith("super::"):
        current = current_rust_module_parts(source, src_root)
        base_parts = current[:-1]
        rest = spec[len("super::") :]
    elif spec.startswith("self::"):
        base_parts = current_rust_module_parts(source, src_root)
        rest = spec[len("self::") :]
    else:
        return ()

    if "::<" in rest:
        rest = rest.split("::<", 1)[0]
    if "::{" in rest:
        rest = rest.split("::{", 1)[0]
    if "," in rest:
        rest = rest.split(",", 1)[0]
    rest = rest.strip("{}")
    parts = base_parts + [part for part in rest.split("::") if part]

    targets: set[str] = set()
    for end in range(len(parts), 0, -1):
        candidate = rust_module_file(src_root, parts[:end])
        if candidate is None:
            continue
        targets.add(as_posix(candidate.relative_to(ROOT)))
        break
    return tuple(sorted(targets))


def normalize_specifier(source: Path, specifier: str) -> tuple[str, ...]:
    if source.suffix == ".rs":
        return normalize_rust_specifier(source, specifier)

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
    if path.suffix == ".rs":
        for match in RUST_USE_RE.finditer(text):
            specifier = match.group(1)
            targets = normalize_specifier(path, specifier)
            if not targets:
                continue
            yield ImportRef(rel, line_number(text, match.start(1)), specifier, targets)
        return

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
