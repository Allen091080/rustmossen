#!/usr/bin/env python3
"""
W52 — Named Plan Files Slice 1 + 2 contract smoke.

Locks the W52 plans.ts additions:

  Slice 1 (pure helper):
    - generatePromptPlanSlug exists and is exported from utils/plans.ts.
    - Output character set is strictly ASCII-safe ([a-z0-9-]).
    - Validation goes through validateWorktreeSlug for worktree/branch/file
      three-way safety.
    - No CJK passthrough rule, no transliteration, no new dependencies.

  Slice 2 (getPlanSlug extension):
    - getPlanSlug accepts an optional second arg `options.firstUserPrompt`.
    - Default call shape `getPlanSlug()` / `getPlanSlug(sessionId)` keeps
      the pre-W52 word-slug behavior (fallback path preserved).
    - copyPlanForResume / copyPlanForFork do NOT invoke
      generatePromptPlanSlug — resumed / forked sessions reuse the slug
      carried by the source log, so existing plans never get renamed.

  Out-of-scope guards (Slice 3 is a separate, single-confirm change):
    - REPL.tsx is NOT modified to thread firstUserPrompt into getPlanSlug.
    - query.ts main loop is NOT modified.
    - tools/EnterWorktreeTool/EnterWorktreeTool.ts caller is unchanged
      (still calls `getPlanSlug()` without options).
    - setup.ts caller is unchanged.
    - No async rename logic introduced.

  Runtime case verification (delegated to wave_w52_runtime_check.ts):
    - Plain English / punctuation / markdown / path-traversal / ANSI /
      overflow / multiple-spaces / trailing-dash collapse cases all
      produce expected ASCII slugs.
    - Empty / whitespace-only / punctuation-only / pure-CJK / emoji-only /
      CJK-dominated / too-short cases all return null (caller falls back
      to generateWordSlug).
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PLANS_TS = ROOT / "utils" / "plans.ts"
WORKTREE_TS = ROOT / "utils" / "worktree.ts"
REPL_TSX = ROOT / "screens" / "REPL.tsx"
QUERY_TS = ROOT / "query.ts"
ENTER_WORKTREE_TS = ROOT / "tools" / "EnterWorktreeTool" / "EnterWorktreeTool.ts"
SETUP_TS = ROOT / "setup.ts"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"
RUNTIME_CHECK = ROOT / "scripts" / "wave_w52_runtime_check.ts"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def check_generate_helper_exists(failures: list[str]) -> None:
    src = read(PLANS_TS)
    if not re.search(
        r"export\s+function\s+generatePromptPlanSlug\s*\(",
        src,
    ):
        failures.append(
            "utils/plans.ts: missing exported generatePromptPlanSlug function"
        )
    if "validateWorktreeSlug" not in src:
        failures.append(
            "utils/plans.ts: must call validateWorktreeSlug for worktree/branch/file safety"
        )
    if "from './worktree.js'" not in src and 'from "./worktree.js"' not in src:
        failures.append(
            "utils/plans.ts: must import validateWorktreeSlug from ./worktree.js"
        )
    # ASCII-safe enforcement: must contain a /[^a-z0-9]+/ collapse OR
    # equivalent pattern. Lock the explicit anchor that v0 ships with.
    if "[^a-z0-9]+" not in src:
        failures.append(
            "utils/plans.ts: ASCII-safe collapse pattern /[^a-z0-9]+/ missing — slug must be a-z 0-9 - only"
        )
    # No CJK passthrough rule. No transliteration. No new deps.
    # Match only on import statements to avoid hitting docstring text that
    # documents the policy.
    for line in src.splitlines():
        stripped = line.strip()
        if not stripped.startswith("import "):
            continue
        if re.search(r"\b(pinyin|kuroshiro|transliter)", stripped, re.IGNORECASE):
            failures.append(
                "utils/plans.ts: must not import phonetic-conversion deps "
                f"(line: {stripped!r})"
            )
            break


def check_get_plan_slug_signature(failures: list[str]) -> None:
    src = read(PLANS_TS)
    # Match the multi-line signature: `getPlanSlug(\n  sessionId?: SessionId,\n  options?: { firstUserPrompt?: string },\n)`
    if not re.search(
        r"export\s+function\s+getPlanSlug\s*\(\s*sessionId\?\s*:\s*SessionId\s*,\s*options\?\s*:\s*\{[^}]*firstUserPrompt\?\s*:\s*string[^}]*\}",
        src,
        re.DOTALL,
    ):
        failures.append(
            "utils/plans.ts: getPlanSlug signature must accept "
            "(sessionId?: SessionId, options?: { firstUserPrompt?: string })"
        )
    # Word-slug fallback path must remain.
    if "generateWordSlug()" not in src:
        failures.append(
            "utils/plans.ts: word-slug fallback (generateWordSlug()) must remain in getPlanSlug"
        )


def check_resume_fork_do_not_use_prompt_slug(failures: list[str]) -> None:
    src = read(PLANS_TS)
    # Locate copyPlanForResume + copyPlanForFork bodies, then assert
    # generatePromptPlanSlug is not called inside them.
    for fn_name in ("copyPlanForResume", "copyPlanForFork"):
        m = re.search(
            rf"export\s+async\s+function\s+{fn_name}\s*\([^)]*\)\s*:\s*Promise<[^>]+>\s*\{{",
            src,
        )
        if not m:
            failures.append(
                f"utils/plans.ts: cannot locate {fn_name} body for prompt-slug-leak check"
            )
            continue
        # Walk braces to find function end.
        depth = 1
        i = m.end()
        while i < len(src) and depth > 0:
            ch = src[i]
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
            i += 1
        body = src[m.end() : i]
        if "generatePromptPlanSlug" in body:
            failures.append(
                f"utils/plans.ts: {fn_name} must NOT call generatePromptPlanSlug "
                "(resumed/forked sessions reuse the source log's slug)"
            )


def check_no_runtime_caller_changes(failures: list[str]) -> None:
    """Slice 3 boundary: no caller has been modified to thread
    firstUserPrompt into getPlanSlug. Lock the existing call shapes."""
    callers = [
        (REPL_TSX, "screens/REPL.tsx"),
        (ENTER_WORKTREE_TS, "tools/EnterWorktreeTool/EnterWorktreeTool.ts"),
        (SETUP_TS, "setup.ts"),
    ]
    for path, label in callers:
        if not path.exists():
            continue
        src = read(path)
        # If a caller touched options.firstUserPrompt that's a Slice 3
        # change, which this PR is forbidden from doing.
        if "firstUserPrompt" in src:
            failures.append(
                f"{label}: must not pass firstUserPrompt — Slice 3 boundary "
                "(REPL/query/worktree integration is a separate, Allen-confirmed slice)"
            )

    # query.ts main loop must not have been touched for plan slug derivation.
    if QUERY_TS.exists():
        src = read(QUERY_TS)
        if "generatePromptPlanSlug" in src:
            failures.append(
                "query.ts: must not import or call generatePromptPlanSlug "
                "in this slice (Slice 3 boundary)"
            )


def check_no_async_rename(failures: list[str]) -> None:
    src = read(PLANS_TS)
    # Async file rename for plan slug would mean fs.rename / renameSync
    # called from getPlanSlug or generatePromptPlanSlug. Lock the absence.
    if re.search(r"\brename(Sync)?\s*\(", src):
        failures.append(
            "utils/plans.ts: must not introduce fs.rename / renameSync — "
            "Slice 1+2 is no-rename design (W52 §2.5 C')"
        )


def check_runtime_check_script(failures: list[str]) -> None:
    if not RUNTIME_CHECK.exists():
        failures.append(
            "scripts/wave_w52_runtime_check.ts: missing — runtime case "
            "verification script must exist alongside this smoke"
        )
        return
    src = read(RUNTIME_CHECK)
    # Lock that the runtime check imports generatePromptPlanSlug from the
    # canonical plans.ts (not a local copy).
    if "from '../utils/plans.js'" not in src and 'from "../utils/plans.js"' not in src:
        failures.append(
            "scripts/wave_w52_runtime_check.ts: must import generatePromptPlanSlug "
            "from '../utils/plans.js' (single source of truth)"
        )


def check_run_all_registration(failures: list[str]) -> None:
    if not RUN_ALL.exists():
        failures.append("scripts/run_all_smoke.sh: missing")
        return
    src = read(RUN_ALL)
    if "wave_w52_named_plan_files_smoke" not in src:
        failures.append(
            "scripts/run_all_smoke.sh: must register wave_w52_named_plan_files_smoke"
        )


def run_runtime_cases(failures: list[str]) -> None:
    bun = shutil.which("bun")
    if bun is None:
        failures.append(
            "bun runtime not found in PATH — cannot run wave_w52_runtime_check.ts"
        )
        return
    proc = subprocess.run(
        [bun, str(RUNTIME_CHECK)],
        cwd=str(ROOT),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=60,
        check=False,
    )
    if proc.returncode != 0:
        failures.append(
            "wave_w52_runtime_check.ts: runtime cases FAILED\n"
            f"--- stdout ---\n{proc.stdout}\n--- stderr ---\n{proc.stderr}"
        )


def main() -> int:
    failures: list[str] = []

    if not PLANS_TS.exists():
        print("FAIL: utils/plans.ts not found", file=sys.stderr)
        return 1

    check_generate_helper_exists(failures)
    check_get_plan_slug_signature(failures)
    check_resume_fork_do_not_use_prompt_slug(failures)
    check_no_runtime_caller_changes(failures)
    check_no_async_rename(failures)
    check_runtime_check_script(failures)
    check_run_all_registration(failures)
    run_runtime_cases(failures)

    print("=== W52 named plan files smoke ===")
    print(f"plans.ts:       {PLANS_TS.relative_to(ROOT)}")
    print(f"runtime check:  {RUNTIME_CHECK.relative_to(ROOT)}")
    print("scope:          Slice 1 (pure helper) + Slice 2 (getPlanSlug optional arg)")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W52 named plan files Slice 1+2 ✓ "
        "(generatePromptPlanSlug pure ASCII-safe, getPlanSlug optional firstUserPrompt, "
        "resume/fork unchanged, no Slice 3 caller drift, no async rename, runtime cases green)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
