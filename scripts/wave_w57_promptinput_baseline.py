#!/usr/bin/env python3
"""W57 A1 — PromptInput perf baseline (static-analysis smoke).

This smoke is intentionally a *baseline*, not an optimisation. It captures
- LOC + file inventory under components/PromptInput/
- the per-keystroke useMemo helpers in PromptInput.tsx that recompute on
  every displayedValue change
- where each helper is implemented (so future optimisation work can target
  exactly one source of truth instead of guessing)
- whether any helper compiles a /g RegExp inside the call (literal-per-call
  is intentional for state-leak safety; see thinking.ts comment), so the
  baseline tells future-us "this is by design, not a regression"

The goal of this round is *to measure, not to change behaviour*. If a future
wave decides to optimise, the same script can be re-run to confirm that the
hot-spot count went down. The smoke fails if any tracked helper disappears
or moves silently.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROMPT_DIR = ROOT / "components" / "PromptInput"
PROMPT_TSX = PROMPT_DIR / "PromptInput.tsx"

# (helper-name, useMemo-line-in-PromptInput.tsx, source-file, source-line, recomputes-on)
TRACKED_HELPERS = [
    ("findThinkingTriggerPositions", "utils/thinking.ts", "displayedValue"),
    ("findBtwTriggerPositions",      "utils/sideQuestion.ts", "displayedValue"),
    ("findBuddyTriggerPositions",    "buddy/useBuddyNotification.tsx", "displayedValue"),
    ("findSlashCommandPositions",    "utils/suggestions/commandSuggestions.ts", "displayedValue + commands"),
    ("findTokenBudgetPositions",     "utils/tokenBudget.ts", "displayedValue (gated by feature flag)"),
    ("findSlackChannelPositions",    "utils/suggestions/slackChannelSuggestions.ts", "displayedValue + knownChannelsVersion"),
    ("parseReferences",              "history.ts", "displayedValue"),
]

EXPECTED_USEMEMO_FOR_HELPERS = {
    "findThinkingTriggerPositions",
    "findBtwTriggerPositions",
    "findBuddyTriggerPositions",
    "findSlashCommandPositions",
    "findTokenBudgetPositions",
    "findSlackChannelPositions",
    "parseReferences",
}

INPUT_SIZES = [1_000, 5_000, 10_000]


def fail(msg: str) -> None:
    print(f"[FAIL] {msg}", file=sys.stderr)
    sys.exit(1)


def info(msg: str) -> None:
    print(msg)


def assert_dir_inventory() -> tuple[int, int]:
    if not PROMPT_DIR.is_dir():
        fail(f"missing dir: {PROMPT_DIR}")
    files = sorted(p for p in PROMPT_DIR.iterdir() if p.is_file() and p.suffix in {".ts", ".tsx"})
    if not files:
        fail("no .ts/.tsx files found under components/PromptInput")
    total_lines = 0
    for f in files:
        total_lines += sum(1 for _ in f.open(encoding="utf-8"))
    info(f"  inventory: {len(files)} files, {total_lines} LOC under components/PromptInput/")
    return len(files), total_lines


def assert_promptinput_size() -> int:
    if not PROMPT_TSX.is_file():
        fail(f"missing file: {PROMPT_TSX}")
    text = PROMPT_TSX.read_text(encoding="utf-8")
    n = text.count("\n") + (0 if text.endswith("\n") else 1)
    if n < 1500:
        fail(f"PromptInput.tsx unexpectedly small ({n} lines) — refactor without baseline update?")
    if n > 4000:
        fail(f"PromptInput.tsx unexpectedly large ({n} lines) — investigate")
    info(f"  PromptInput.tsx: {n} lines (expected 1500-4000)")
    return n


def assert_helpers_called_in_usememo() -> dict[str, int]:
    text = PROMPT_TSX.read_text(encoding="utf-8")
    locations: dict[str, int] = {}
    for helper in EXPECTED_USEMEMO_FOR_HELPERS:
        # find every line containing helper(  — record first one
        first_line = None
        for i, line in enumerate(text.splitlines(), start=1):
            if f"{helper}(" in line:
                first_line = i
                # also assert the surrounding context contains useMemo within
                # 4 lines above (the call sits inside `useMemo(() => helper(...))`)
                start = max(0, i - 4)
                window = "\n".join(text.splitlines()[start:i + 1])
                if "useMemo" not in window:
                    fail(f"{helper} call at PromptInput.tsx:{i} not within a useMemo window")
                break
        if first_line is None:
            fail(f"{helper} no longer called from PromptInput.tsx — baseline drift")
        locations[helper] = first_line
    return locations


def assert_helper_sources_exist() -> None:
    seen: set[str] = set()
    for name, rel, _ in TRACKED_HELPERS:
        path = ROOT / rel
        if not path.is_file():
            fail(f"helper source missing: {rel}")
        body = path.read_text(encoding="utf-8")
        if f"export function {name}" not in body and f"function {name}" not in body:
            fail(f"helper '{name}' not found in {rel}")
        seen.add(name)
    missing = EXPECTED_USEMEMO_FOR_HELPERS - seen - {"parseReferences"}
    if missing:
        fail(f"tracked helpers without source mapping: {missing}")


def assert_per_call_regex_pattern() -> int:
    """Each find* helper compiles a fresh /g regex per call. This is
    intentional — see thinking.ts comment about matchAll lastIndex leakage.
    Baseline records the count so future optimisation can audit whether a
    cached regex is ever introduced (which would be a behaviour change to
    review)."""
    files_with_local_regex = 0
    helper_files = {ROOT / rel for _, rel, _ in TRACKED_HELPERS}
    pattern = re.compile(r"matchAll\(/[^/]+/[gimsuy]*\)|new RegExp\(|\.exec\(text\)")
    for f in helper_files:
        if pattern.search(f.read_text(encoding="utf-8")):
            files_with_local_regex += 1
    info(f"  per-call-regex helpers: {files_with_local_regex}/{len(helper_files)} (intentional)")
    return files_with_local_regex


def assert_input_size_baseline() -> None:
    """We don't run a real perf benchmark from a static smoke; we record the
    input sizes the baseline is *intended to cover*, so a future wave that
    introduces an actual perf harness can re-use the same shape."""
    info(f"  input-size baseline (for future perf harness): {INPUT_SIZES} chars")


def assert_no_behaviour_drift() -> None:
    text = PROMPT_TSX.read_text(encoding="utf-8")
    # The displayedValue selector is the bottleneck. It must remain memoised.
    if "const displayedValue = useMemo(" not in text:
        fail("displayedValue is no longer memoised — baseline must be refreshed")
    # ultraplanTriggers is the existing dead-array (W55-era cleanup). If
    # someone re-enables it without a new useMemo we want to know.
    if "const ultraplanTriggers" in text and "useMemo" in text.split("const ultraplanTriggers", 1)[1].splitlines()[0]:
        fail("ultraplanTriggers reverted to a useMemo — investigate regression")


def main() -> int:
    info("W57 A1 — PromptInput perf baseline")
    info("=" * 60)

    file_count, loc = assert_dir_inventory()
    promptinput_lines = assert_promptinput_size()
    locations = assert_helpers_called_in_usememo()
    assert_helper_sources_exist()
    regex_files = assert_per_call_regex_pattern()
    assert_input_size_baseline()
    assert_no_behaviour_drift()

    info("")
    info("hot-spot helpers (per-keystroke useMemo recompute):")
    for name, source, deps in TRACKED_HELPERS:
        line = locations.get(name, "?")
        info(f"  - {name:34s} PromptInput.tsx:{line:<5} <- {source}  [{deps}]")

    info("")
    info("[PASS] W57 A1 baseline — 7 hot-spot helpers tracked, sources exist, "
         f"PromptInput.tsx={promptinput_lines}L, dir={file_count}f/{loc}L")
    return 0


if __name__ == "__main__":
    sys.exit(main())
