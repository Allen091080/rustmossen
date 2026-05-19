#!/usr/bin/env python3
"""
typecheck_diff.py — run tsc and fail only if NEW errors appear vs baseline.

The repo has 1478 pre-existing TS errors (mostly .js module resolution noise
and implicit-any warnings) that are too costly to fix retroactively but should
not be allowed to grow. This wrapper:

  1. Runs `bun run typecheck`
  2. Captures all `error TS` lines, sorts them
  3. Diffs against `.mossensrc/typecheck-baseline.txt`
  4. Exits 0 if current ⊆ baseline (no new errors)
  5. Exits non-zero with a diff report if new errors appear

To regenerate baseline (after intentional cleanup):

  bun run typecheck 2>&1 | grep "error TS" | sort > scripts/typecheck-baseline.txt
"""

from __future__ import annotations

import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "scripts" / "typecheck-baseline.txt"

# Normalize by stripping line:col so insertions/deletions don't false-positive.
# Keeps: file + error code + message → still uniquely identifies most errors.
LINE_COL_RE = re.compile(r"\((\d+),(\d+)\)")

# Normalize project-absolute paths so the same baseline works in the main
# workspace AND any sibling worktree (e.g. mossensrc-otel-removal). Without
# this, type-expansion errors that embed absolute import paths look "new"
# when run from a worktree with a different directory name.
PROJECT_PATH_RE = re.compile(r"/[A-Za-z0-9_./-]+?/mossensrc(?:-[A-Za-z0-9_-]+)?(?=/)")

# Normalize TS type-expansion field counts ("... 85 more ...") so adding a
# single Zod field doesn't change the error count vs baseline.
TS_TRUNC_COUNT_RE = re.compile(r"\.\.\. \d+ more \.\.\.")

# Normalize union-type member ordering inside quoted type renders. TS may
# emit `'A | B | C'` in different member orders across runs depending on
# module resolution order — same logical type, different text. Sort so
# the baseline is stable.
QUOTED_TYPE_RE = re.compile(r"'([^']+)'")


def _sort_union(text: str) -> str:
    if " | " not in text:
        return text
    parts = text.split(" | ")
    return " | ".join(sorted(parts))


def normalize(line: str) -> str:
    """Strip (line,col), project paths, TS truncation counts, and sort union members."""
    line = LINE_COL_RE.sub("(L,C)", line)
    line = PROJECT_PATH_RE.sub("<REPO>", line)
    line = TS_TRUNC_COUNT_RE.sub("... N more ...", line)
    line = QUOTED_TYPE_RE.sub(lambda m: "'" + _sort_union(m.group(1)) + "'", line)
    return line


def to_counter(lines: list[str]) -> Counter[str]:
    """Convert list of error lines to Counter (so duplicates of same error
    in different positions count as multiple instances)."""
    return Counter(normalize(line) for line in lines)


def main() -> int:
    if not BASELINE.exists():
        print(f"missing baseline: {BASELINE}", file=sys.stderr)
        print("regenerate with:", file=sys.stderr)
        print("  bun run typecheck 2>&1 | grep 'error TS' | sort > scripts/typecheck-baseline.txt", file=sys.stderr)
        return 2

    baseline_lines = [l for l in BASELINE.read_text().splitlines() if l]
    baseline_counter = to_counter(baseline_lines)
    proc = subprocess.run(
        ["bun", "run", "typecheck"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    combined = (proc.stdout or "") + (proc.stderr or "")
    current_lines = [line for line in combined.splitlines() if "error TS" in line]
    current_counter = to_counter(current_lines)

    # Counter subtraction: only retains positive deltas
    new_delta = current_counter - baseline_counter
    fixed_delta = baseline_counter - current_counter

    new_errors = []
    for line in current_lines:
        norm = normalize(line)
        if new_delta[norm] > 0:
            new_errors.append(line)
            new_delta[norm] -= 1

    print(f"typecheck baseline: {len(baseline_lines)} errors")
    print(f"typecheck current : {len(current_lines)} errors")
    fixed_count = sum(fixed_delta.values())
    if fixed_count:
        print(f"typecheck fixed   : {fixed_count} (great — consider regenerating baseline)")

    if new_errors:
        print(f"\n❌ {len(new_errors)} NEW typecheck error(s) introduced:\n")
        for e in new_errors[:50]:
            print(f"  {e}")
        if len(new_errors) > 50:
            print(f"  ... and {len(new_errors) - 50} more")
        return 1

    print("\n✅ no new typecheck errors")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
