#!/usr/bin/env python3
"""W57 — second-tier closure consolidated smoke.

Asserts the W57 wave deliverables are present and consistent:
  - C1  /effort interactive picker (EffortPicker.tsx + router wiring)
  - A1  PromptInput perf baseline   (scripts/wave_w57_promptinput_baseline.py)
  - A2  /resume token baseline      (scripts/wave_w57_resume_token_baseline.py)
  - C7  Vim visual mode             (DEFERRED — archive doc records reason)
  - D6  audit log writer            (DEFERRED — repo MUST NOT contain a writer
                                     module, smoke, or caller; the next wave
                                     D6-A must land redaction + schema first)

Then chains the three implementation smokes (effort_picker /
promptinput_baseline / resume_token_baseline) so a single command covers
the wave. C7 and D6 are deferred — there are no child smokes for them, by
design. The D6 absence checks are STRUCTURAL: if anyone reintroduces a
writer module without going through D6-A, this smoke fails.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

EXPECTED_DELIVERABLES = [
    ("C1 effort picker UI",       ROOT / "commands" / "effort" / "EffortPicker.tsx"),
    ("C1 effort router",          ROOT / "commands" / "effort" / "effort.tsx"),
    ("A1 promptinput baseline",   ROOT / "scripts" / "wave_w57_promptinput_baseline.py"),
    ("A2 resume token baseline",  ROOT / "scripts" / "wave_w57_resume_token_baseline.py"),
    ("W57 archive doc",           ROOT / "docs" / "upgrade" / "W57-second-tier-closure.md"),
]

# Files / paths that MUST NOT exist — the D6 deferral red line.
FORBIDDEN_PATHS = [
    ROOT / "utils" / "auditLog.ts",
    ROOT / "utils" / "audit_log.ts",
    ROOT / "utils" / "audit" / "writer.ts",
    ROOT / "scripts" / "wave_w57_audit_log_writer_smoke.py",
]

# Identifiers that, if found anywhere outside this smoke + the archive doc,
# indicate someone reintroduced the deferred writer surface.
FORBIDDEN_IDENTIFIERS = [
    "writeAuditRecord",
    "AuditEventKind",
    "AuditRecord",
    "isAuditLogEnabled",
    "getAuditLogDir",
    "getAuditLogFileForDate",
    "MOSSEN_CODE_AUDIT_LOG",
]

# Doc-only callers OK; everywhere else is a regression.
ALLOWED_DOC_REFERENCES = {
    ROOT / "docs" / "upgrade" / "W57-second-tier-closure.md",
}

CHILD_SMOKES = [
    "scripts/wave_w57_effort_picker_smoke.py",
    "scripts/wave_w57_promptinput_baseline.py",
    "scripts/wave_w57_resume_token_baseline.py",
]


def fail(msg: str) -> None:
    print(f"[FAIL] {msg}", file=sys.stderr)
    sys.exit(1)


def info(msg: str) -> None:
    print(msg)


def assert_deliverables_exist() -> None:
    missing = [(label, p) for label, p in EXPECTED_DELIVERABLES if not p.is_file()]
    if missing:
        rows = "\n".join(f"    - {label}: {p.relative_to(ROOT)}" for label, p in missing)
        fail(f"W57 deliverables missing:\n{rows}")
    info(f"  deliverables: {len(EXPECTED_DELIVERABLES)}/{len(EXPECTED_DELIVERABLES)} present")


def assert_d6_writer_absent() -> None:
    """The D6 red line: no writer module, no caller, no env var, no smoke."""
    present = [p for p in FORBIDDEN_PATHS if p.exists()]
    if present:
        rows = "\n".join(f"    - {p.relative_to(ROOT)}" for p in present)
        fail(f"D6 deferred but writer files reintroduced:\n{rows}")
    info(f"  D6 writer module: absent ({len(FORBIDDEN_PATHS)} forbidden paths checked)")


def assert_no_d6_identifiers_in_code() -> None:
    """Grep the entire repo for the deferred writer's identifiers. Any
    occurrence outside docs is a regression."""
    offenders: list[tuple[Path, str]] = []
    skip_dirs = {"node_modules", ".bun", ".git", "dist", "build"}
    for ext in ("*.ts", "*.tsx", "*.js", "*.py"):
        for path in ROOT.rglob(ext):
            if any(part in skip_dirs for part in path.parts):
                continue
            if path == Path(__file__):
                continue
            try:
                text = path.read_text(encoding="utf-8")
            except (UnicodeDecodeError, OSError):
                continue
            for ident in FORBIDDEN_IDENTIFIERS:
                if ident in text:
                    offenders.append((path.relative_to(ROOT), ident))
    if offenders:
        rows = "\n".join(f"    - {p}: {ident}" for p, ident in offenders)
        fail(f"D6 identifiers found in code (deferred wave):\n{rows}")
    info(f"  D6 identifiers: 0 hits in code ({len(FORBIDDEN_IDENTIFIERS)} names checked)")


def assert_archive_doc_records_c7_and_d6_defer() -> None:
    doc = ROOT / "docs" / "upgrade" / "W57-second-tier-closure.md"
    text = doc.read_text(encoding="utf-8")

    if "C7" not in text:
        fail("archive doc does not mention C7")
    if "PromptInput" not in text:
        fail("archive doc must explain WHY C7 was deferred (combinedHighlights/PromptInput)")

    if "D6" not in text:
        fail("archive doc does not mention D6")
    # D6 must explicitly carry the deferral wording.
    if not re.search(r"D6[^\n]*[Dd]efer", text) and "D6 — Local audit-log writer (DEFERRED)" not in text:
        fail("archive doc must mark D6 as deferred")
    # D6-A prerequisites — five named gates.
    for must in ("redaction", "schema", "caller", "rotat", "view"):
        if must.lower() not in text.lower():
            fail(f"D6 deferral block missing required prerequisite keyword: {must!r}")
    info("  archive doc: C7 + D6 both deferred with reasons + D6-A prereqs captured")


def assert_no_new_local_log_writers() -> None:
    """Belt-and-braces: D6 is "no new local-log writer". Catch any
    appendFile/createWriteStream that landed under utils/ alongside the
    audit name space — independent of the identifier list above."""
    suspects: list[tuple[Path, str]] = []
    skip_dirs = {"node_modules", ".bun", ".git", "dist", "build"}
    pattern = re.compile(r"\b(appendFile|createWriteStream|appendFileSync)\s*\(")
    # Only scan paths that were not in the repo prior to this wave (best-effort
    # heuristic: untracked or in commands/effort/ + utils/auditLog* + W57 smoke).
    candidates = [
        ROOT / "utils" / "auditLog.ts",
        ROOT / "utils" / "audit",
    ]
    for c in candidates:
        if c.exists():
            if c.is_file():
                text = c.read_text(encoding="utf-8")
                if pattern.search(text):
                    suspects.append((c.relative_to(ROOT), "writer api"))
            else:
                for f in c.rglob("*.ts"):
                    if any(part in skip_dirs for part in f.parts):
                        continue
                    text = f.read_text(encoding="utf-8")
                    if pattern.search(text):
                        suspects.append((f.relative_to(ROOT), "writer api"))
    if suspects:
        rows = "\n".join(f"    - {p}: {kind}" for p, kind in suspects)
        fail(f"new local-log writer detected (D6 deferred):\n{rows}")
    info("  no new local-log writer paths in audit namespace")


def run_child(path: str) -> None:
    info(f"--- {path} ---")
    result = subprocess.run(
        [sys.executable, path],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        timeout=120,
    )
    sys.stdout.write(result.stdout)
    sys.stdout.write(result.stderr)
    if result.returncode != 0:
        fail(f"child smoke failed: {path} (exit {result.returncode})")


def main() -> int:
    info("W57 — second-tier closure consolidated smoke")
    info("=" * 60)
    assert_deliverables_exist()
    assert_d6_writer_absent()
    assert_no_d6_identifiers_in_code()
    assert_no_new_local_log_writers()
    assert_archive_doc_records_c7_and_d6_defer()
    info("")
    for child in CHILD_SMOKES:
        run_child(child)
    info("")
    info("[PASS] W57 — implemented C1/A1/A2 + deferred C7/D6 (D6 writer absent, no callers)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
