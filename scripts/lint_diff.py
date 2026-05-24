#!/usr/bin/env python3
"""
lint_diff.py — run eslint and fail only if NEW errors appear vs baseline.

Same delta-gate philosophy as typecheck_diff.py. Repo has ~1300 pre-existing
lint problems (39 errors, ~1300 warnings) mostly from upstream Mossen Code
(react-hooks rules-of-hooks violations, console statements, unused vars).

  1. Runs `bun run lint`
  2. Captures all error/warning lines normalized to `file:line:col rule`
  3. Diffs against `scripts/lint-baseline.txt`
  4. Exits 0 if current ⊆ baseline (no new lint errors)
  5. Exits non-zero with new entries when delta appears

To regenerate baseline:

  bun run lint 2>&1 | python3 scripts/lint_diff.py --regenerate
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "scripts" / "lint-baseline.txt"

# eslint output:
#   /abs/path/to/file.ts
#     12:5  error    Some message  rule-name
#     34:1  warning  Other         other/rule
LINE_RE = re.compile(r"^\s+(\d+):(\d+)\s+(error|warning)\s+(.+?)\s{2,}([\w@/\-]+)\s*$")


def normalize_eslint_output(text: str) -> list[str]:
    """Convert raw eslint output to (file, severity, rule) tuples — drops line:col
    so insertions/deletions don't false-positive. Each tuple counted as a token,
    so duplicates of same (file, rule) at different lines remain distinguishable
    via Counter."""
    out: list[str] = []
    current_file = ""
    for raw_line in text.splitlines():
        if raw_line.startswith("/"):
            current_file = str(Path(raw_line).relative_to(ROOT)) if raw_line.startswith(str(ROOT)) else raw_line
            continue
        m = LINE_RE.match(raw_line)
        if not m:
            continue
        _line, _col, severity, _msg, rule = m.groups()
        out.append(f"{current_file}\t{severity}\t{rule}")
    return sorted(out)  # keep duplicates (Counter-based diff)


def run_lint() -> str:
    proc = subprocess.run(
        ["bun", "run", "lint"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    return (proc.stdout or "") + (proc.stderr or "")


def main() -> int:
    if "--regenerate" in sys.argv:
        text = run_lint()
        lines = normalize_eslint_output(text)
        BASELINE.write_text("\n".join(lines) + "\n")
        print(f"baseline regenerated: {len(lines)} entries → {BASELINE}")
        return 0

    if not BASELINE.exists():
        print(f"missing baseline: {BASELINE}", file=sys.stderr)
        print("regenerate with: python3 scripts/lint_diff.py --regenerate", file=sys.stderr)
        return 2

    from collections import Counter
    baseline_lines = [l for l in BASELINE.read_text().splitlines() if l]
    baseline_counter = Counter(baseline_lines)
    current_lines = normalize_eslint_output(run_lint())
    current_counter = Counter(current_lines)

    new_delta = current_counter - baseline_counter
    fixed_delta = baseline_counter - current_counter

    new_problems = []
    for line in current_lines:
        if new_delta[line] > 0:
            new_problems.append(line)
            new_delta[line] -= 1

    print(f"lint baseline: {len(baseline_lines)} entries")
    print(f"lint current : {len(current_lines)} entries")
    fixed_count = sum(fixed_delta.values())
    if fixed_count:
        print(f"lint fixed   : {fixed_count} (great — consider regenerating baseline)")

    if new_problems:
        print(f"\n❌ {len(new_problems)} NEW lint problem(s) introduced:\n")
        for p in new_problems[:50]:
            print(f"  {p}")
        if len(new_problems) > 50:
            print(f"  ... and {len(new_problems) - 50} more")
        return 1

    print("\n✅ no new lint problems")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
