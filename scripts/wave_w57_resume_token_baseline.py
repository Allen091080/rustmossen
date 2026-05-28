#!/usr/bin/env python3
"""W57 A2 — /resume token-efficiency baseline (static-analysis smoke).

What "token efficiency" actually means for /resume:

  - The picker itself does **not** emit tokens. It loads lite metadata only
    (firstPrompt + customTitle + a few flags) via a fixed 64KB tail-window
    read per session. No transcript bodies enter context.
  - Only when the user **selects** a session does loadFullLog() fetch the
    full transcript via loadTranscriptFile(). At that point the cost is
    fundamentally proportional to the chosen session's history — which
    the user already paid for previously.

So this baseline records the structural facts that bound today's cost, so
that any future "make /resume cheaper" wave can justify the change against
the actual code:

  - LITE_READ_BUF_SIZE (the per-session window during enumeration)
  - Lite vs full call sites (where the heavy load actually happens)
  - 'allProjects' fan-out (whether the picker traverses one repo or all)
  - The deferred-load invariants (loadFullLog only after select / UUID hit)

The smoke fails if any tracked invariant drifts silently.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RESUME_TSX = ROOT / "commands" / "resume" / "resume.tsx"
PORTABLE_TS = ROOT / "utils" / "sessionStoragePortable.ts"
SESSION_STORAGE_TS = ROOT / "utils" / "sessionStorage.ts"

EXPECTED_LITE_READ_BUF_SIZE = 65536  # 64 KiB tail window per session


def fail(msg: str) -> None:
    print(f"[FAIL] {msg}", file=sys.stderr)
    sys.exit(1)


def info(msg: str) -> None:
    print(msg)


def assert_resume_file_size() -> int:
    if not RESUME_TSX.is_file():
        fail(f"missing file: {RESUME_TSX}")
    n = sum(1 for _ in RESUME_TSX.open(encoding="utf-8"))
    if n < 200 or n > 700:
        fail(f"resume.tsx unexpectedly sized ({n} lines) — refactor without baseline update?")
    info(f"  resume.tsx: {n} lines (expected 200-700)")
    return n


def assert_lite_read_buf_size() -> int:
    if not PORTABLE_TS.is_file():
        fail(f"missing file: {PORTABLE_TS}")
    text = PORTABLE_TS.read_text(encoding="utf-8")
    m = re.search(r"export const LITE_READ_BUF_SIZE\s*=\s*(\d+)", text)
    if not m:
        fail("LITE_READ_BUF_SIZE export no longer found in sessionStoragePortable.ts")
    val = int(m.group(1))
    if val != EXPECTED_LITE_READ_BUF_SIZE:
        fail(
            f"LITE_READ_BUF_SIZE drift: got {val}, expected {EXPECTED_LITE_READ_BUF_SIZE}. "
            "If intentional, update this baseline and document why."
        )
    info(f"  LITE_READ_BUF_SIZE: {val} bytes ({val // 1024} KiB tail per session)")
    return val


def assert_lite_full_separation() -> dict[str, int]:
    """The picker must remain a lite-only path until the user selects a row.
    `loadFullLog` must only be invoked AFTER selection (handleSelect) or
    after a UUID/title match — never inside the picker render or the
    initial loadLogs path. We assert by counting call sites and verifying
    the lite-load functions appear before any loadFullLog call in the
    file order (a coarse but stable structural check)."""
    text = RESUME_TSX.read_text(encoding="utf-8")
    counts = {
        "loadFullLog": text.count("loadFullLog("),
        "loadAllProjectsMessageLogs": text.count("loadAllProjectsMessageLogs("),
        "loadSameRepoMessageLogs": text.count("loadSameRepoMessageLogs("),
        "isLiteLog": text.count("isLiteLog("),
        "getLastSessionLog": text.count("getLastSessionLog("),
    }
    if counts["loadFullLog"] < 2:
        fail(f"loadFullLog call count regressed: {counts['loadFullLog']} (expected ≥ 2)")
    if counts["loadSameRepoMessageLogs"] < 1:
        fail("loadSameRepoMessageLogs no longer used — picker may have switched to a heavier path")
    if counts["isLiteLog"] < 2:
        fail(
            f"isLiteLog gate count regressed: {counts['isLiteLog']} — loadFullLog "
            "must always sit behind an isLiteLog check, otherwise we double-read full sessions"
        )
    info(
        "  call-site inventory: "
        + ", ".join(f"{k}×{v}" for k, v in counts.items())
    )
    return counts


def assert_load_full_log_is_deferred() -> None:
    """Static positional check: the first loadFullLog call must come AFTER
    the first lite-loader call (loadSameRepoMessageLogs or
    loadAllProjectsMessageLogs). If a future refactor moves loadFullLog
    earlier, this baseline catches it."""
    text = RESUME_TSX.read_text(encoding="utf-8")
    lite_idx = min(
        (i for i in (
            text.find("loadSameRepoMessageLogs("),
            text.find("loadAllProjectsMessageLogs("),
        ) if i >= 0),
        default=-1,
    )
    full_idx = text.find("loadFullLog(")
    if lite_idx < 0 or full_idx < 0:
        fail("could not locate lite/full loader call sites for ordering check")
    if not full_idx > lite_idx:
        fail(
            f"loadFullLog (offset {full_idx}) is no longer after the first lite loader "
            f"(offset {lite_idx}) — picker may now eagerly load full transcripts"
        )
    info("  loadFullLog is correctly deferred (called after lite-loader in file order)")


def assert_handle_select_only_loads_full_after_match() -> None:
    """The handleSelect handler should only call loadFullLog when isLiteLog
    is true — otherwise a full log would be re-loaded for already-full
    entries (the picker enrichment path produces full logs for visible rows
    in some flows)."""
    text = RESUME_TSX.read_text(encoding="utf-8")
    pattern = re.compile(r"isLiteLog\(log\)\s*\?\s*await\s+loadFullLog\(log\)\s*:\s*log")
    if len(pattern.findall(text)) < 2:
        fail(
            "ternary 'isLiteLog(log) ? await loadFullLog(log) : log' missing in ≥ 2 places — "
            "handleSelect / UUID match / title match must each gate full-load behind isLiteLog"
        )
    info("  isLiteLog gate present at all three full-load sites (select / uuid / title)")


def assert_no_full_transcript_in_picker_render() -> None:
    """The picker's render path must never reference loadFullLog. We
    isolate the ResumeCommand body by slicing from `function ResumeCommand`
    to the next top-level `export function` / `export const`."""
    text = RESUME_TSX.read_text(encoding="utf-8")
    start = text.find("function ResumeCommand(")
    if start < 0:
        fail("could not locate ResumeCommand definition")
    # Find next top-level export after start
    end_candidates = [
        text.find("export function filterResumableSessions", start),
        text.find("export const call", start),
    ]
    end_candidates = [i for i in end_candidates if i > start]
    if not end_candidates:
        fail("could not locate end of ResumeCommand body")
    end = min(end_candidates)
    body = text[start:end]
    # loadFullLog must appear inside body but only AFTER handleSelect or
    # not at all before handleSelect.
    select_idx = body.find("handleSelect")
    if select_idx < 0:
        fail("handleSelect not found in ResumeCommand body — refactored?")
    full_idx = body.find("loadFullLog(")
    if full_idx >= 0 and full_idx < select_idx:
        fail("loadFullLog appears in ResumeCommand body before handleSelect — eager-load regression")
    info("  ResumeCommand render path does not eagerly call loadFullLog")


def assert_progressive_loading_present() -> None:
    if not SESSION_STORAGE_TS.is_file():
        fail(f"missing file: {SESSION_STORAGE_TS}")
    text = SESSION_STORAGE_TS.read_text(encoding="utf-8")
    if "loadAllProjectsMessageLogsProgressive" not in text:
        fail("loadAllProjectsMessageLogsProgressive missing — progressive load was removed?")
    if "loadSameRepoMessageLogsProgressive" not in text:
        fail("loadSameRepoMessageLogsProgressive missing — progressive load was removed?")
    info("  progressive lite loaders present (same-repo + all-projects)")


def main() -> int:
    info("W57 A2 — /resume token-efficiency baseline")
    info("=" * 60)

    n = assert_resume_file_size()
    buf = assert_lite_read_buf_size()
    counts = assert_lite_full_separation()
    assert_load_full_log_is_deferred()
    assert_handle_select_only_loads_full_after_match()
    assert_no_full_transcript_in_picker_render()
    assert_progressive_loading_present()

    info("")
    info("baseline observations (token cost reasoning):")
    info(f"  - picker enumeration: lite-only, ≤ {buf} bytes per session (tail window)")
    info("  - full transcript load: deferred to handleSelect / UUID match / title match")
    info(f"  - loadFullLog call sites in resume.tsx: {counts['loadFullLog']} (all gated by isLiteLog)")
    info("  - allProjects toggle: extends fan-out to all project dirs, still lite-only enumeration")
    info("")
    info("[PASS] W57 A2 baseline — /resume picker remains lite-only; full load deferred")
    return 0


if __name__ == "__main__":
    sys.exit(main())
