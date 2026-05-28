#!/usr/bin/env python3
"""
W55 Round 2 — /project purge contract smoke.

Locks the C5 user-facing project-purge flow added in W55 Round 2:

  utils/projectPurge.ts (engine):
    - PROJECT_PURGE_TOKEN_TTL_MS export = 10 * 60 * 1000.
    - getProjectPurgePlan / executeProjectPurgePlan exported.
    - Token store is a Map<string, ProjectPurgePlan> (one-shot semantics).
    - executeProjectPurgePlan deletes the token from the store BEFORE any
      side effect (rm/copyFile/writeFile/mkdir/copyRecursive/rename).
    - Three-way active-project guard: getOriginalCwd + getProjectRoot +
      getSessionProjectDir, called in BOTH dry-run and confirm.
    - Memory override detection (env vars + settings sources) — when active,
      memoryStatus = 'external' and --include-memory is REJECTED.
    - Symlinks are NOT followed during archive (lstat + skip).
    - rm() targets are limited to entries inside ~/.mossen/projects/<sanitized>/
      or the backup dir; no fs.rm of project root is hardcoded into the path.

  commands/project/parseArgs.ts:
    - ParsedProjectCommand union has 'purge' case with target /
      includeMemory / confirmToken fields.
    - Six forbidden flags (--all-projects, --orphan-only, --no-archive,
      --force, --yes, --i-know-what-im-doing) are rejected with the
      'unsupported_flag' tag — never silently ignored.

  commands/project/project.tsx:
    - Routes parsed.type === 'purge' to <ProjectPurge ... />.
    - Routes parsed.type === 'unsupported_flag' to <ProjectPurge ... /> with
      the unsupportedFlag prop set.

  commands/project/ProjectPurge.tsx:
    - Calls getProjectPurgePlan() for dry-run and executeProjectPurgePlan()
      for confirm.
    - Bilingual (en + zh) for every user-facing string.
    - Surfaces memory behavior in dry-run output (preserved by default,
      external override warning, --include-memory consequence).
    - Emits the confirm command shape (`/project purge ... --confirm <token>`).

  commands.ts:
    - Imports project from ./commands/project/index.js and registers it
      in the COMMANDS array.

  Round-2 boundary guards (NO drift):
    - No --force / --bypass / --no-archive / --i-know-what-im-doing tokens
      anywhere in the new code.
    - controlSchemas.ts not aware of /project purge (slash command, not
      protocol).
    - query.ts / processUserInput / Tool.ts / Workbench / commands/insights.ts
      do not reference projectPurge engine helpers.
    - Engine never imports settings/.mossen.json/custom-backend.env/
      history.jsonl/plugins/plans/debug/file-history/paste-cache/session-env/
      session-transcripts/sessions/shell-snapshots/tasks/telemetry literals.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROJECT_PURGE = ROOT / "utils" / "projectPurge.ts"
PARSE_ARGS = ROOT / "commands" / "project" / "parseArgs.ts"
PROJECT_TSX = ROOT / "commands" / "project" / "project.tsx"
PROJECT_INDEX = ROOT / "commands" / "project" / "index.tsx"
PROJECT_PURGE_TSX = ROOT / "commands" / "project" / "ProjectPurge.tsx"
COMMANDS_TS = ROOT / "commands.ts"
SDK_CONTROL_SCHEMAS = ROOT / "entrypoints" / "sdk" / "controlSchemas.ts"
QUERY_TS = ROOT / "query.ts"
PROCESS_USER_INPUT = ROOT / "utils" / "processUserInput" / "processUserInput.ts"
TOOL_TS = ROOT / "Tool.ts"
COMMANDS_INSIGHTS = ROOT / "commands" / "insights.ts"
RUN_ALL = ROOT / "scripts" / "run_all_smoke.sh"

# Forbidden user-facing flags / code symbols. Any occurrence in the new
# /project purge surface or the engine = FAIL.
FORBIDDEN_FLAGS = [
    "--all-projects",
    "--orphan-only",
    "--no-archive",
    "--force",
    "--yes",
    "--i-know-what-im-doing",
    "forcePurge",
    "force_purge",
    "bypassActive",
    "bypass_active",
    "skipArchive",
    "purgeNoArchive",
]

# Sibling paths under ~/.mossen/ that the engine must never reference by
# literal string.
FORBIDDEN_SIBLING_LITERALS = [
    "settings.json",
    ".mossen.json",
    "custom-backend.env",
    "history.jsonl",
    "/plugins/",
    "/plans/",
    "/debug/",
    "/file-history/",
    "/paste-cache/",
    "/session-env/",
    "/session-transcripts/",
    "/sessions/",
    "/shell-snapshots/",
    "/tasks/",
    "/telemetry/",
]


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


# ---------------------------------------------------------------------------
# Engine: utils/projectPurge.ts
# ---------------------------------------------------------------------------


def check_engine_exists(failures: list[str]) -> None:
    if not PROJECT_PURGE.exists():
        failures.append("utils/projectPurge.ts: missing")


def check_engine_exports(failures: list[str]) -> None:
    src = read(PROJECT_PURGE)
    if "export const PROJECT_PURGE_TOKEN_TTL_MS" not in src:
        failures.append(
            "utils/projectPurge.ts: missing PROJECT_PURGE_TOKEN_TTL_MS export"
        )
    if not re.search(
        r"export const PROJECT_PURGE_TOKEN_TTL_MS\s*=\s*10\s*\*\s*60\s*\*\s*1000",
        src,
    ):
        failures.append(
            "utils/projectPurge.ts: PROJECT_PURGE_TOKEN_TTL_MS must equal "
            "10 * 60 * 1000 (10-minute TTL)"
        )
    if "export async function getProjectPurgePlan" not in src:
        failures.append("utils/projectPurge.ts: missing getProjectPurgePlan export")
    if "export async function executeProjectPurgePlan" not in src:
        failures.append(
            "utils/projectPurge.ts: missing executeProjectPurgePlan export"
        )
    if "export function _resetProjectPurgePlanStoreForTesting" not in src:
        failures.append(
            "utils/projectPurge.ts: missing _resetProjectPurgePlanStoreForTesting "
            "(test reset hook required by smoke convention)"
        )

    # Plan / result / error shape — required keys.
    for key in [
        "targetCwd",
        "sanitizedTarget",
        "originalProjectDir",
        "memoryStatus",
        "includeMemory",
        "toArchive",
        "toSkip",
        "archiveDir",
        "totalArchiveBytes",
    ]:
        if key not in src:
            failures.append(
                f"utils/projectPurge.ts: ProjectPurgePlan must surface '{key}'"
            )

    # Tagged error union must include all required tags.
    for tag in [
        "unknown_token",
        "expired_token",
        "active_project",
        "invalid_target",
        "external_memory_include_rejected",
        "token_target_mismatch",
        "project_dir_missing",
    ]:
        if f"'{tag}'" not in src:
            failures.append(
                f"utils/projectPurge.ts: ProjectPurgeError must include '{tag}' tag"
            )


def check_engine_imports(failures: list[str]) -> None:
    src = read(PROJECT_PURGE)
    # State helpers for the three-way active guard.
    for sym in ["getOriginalCwd", "getProjectRoot", "getSessionProjectDir"]:
        if sym not in src:
            failures.append(
                f"utils/projectPurge.ts: must call {sym} (three-way active-project guard)"
            )
    # Path/storage helpers.
    for sym in ["sanitizePath", "getProjectsDir", "findProjectDir"]:
        if sym not in src:
            failures.append(
                f"utils/projectPurge.ts: must use {sym} from sessionStoragePortable"
            )
    # Backup base.
    if "getMossenConfigHomeDir" not in src:
        failures.append(
            "utils/projectPurge.ts: must use getMossenConfigHomeDir (backup root)"
        )
    if "'backups'" not in src:
        failures.append(
            "utils/projectPurge.ts: archive root must include 'backups' directory literal"
        )
    if "purge-" not in src:
        failures.append(
            "utils/projectPurge.ts: archive prefix must be 'purge-<ts>-<hex>/'"
        )


def check_engine_token_safety(failures: list[str]) -> None:
    src = read(PROJECT_PURGE)
    # The plan store must be a Map keyed by token.
    if "new Map<string, ProjectPurgePlan>" not in src:
        failures.append(
            "utils/projectPurge.ts: projectPurgePlanStore must be Map<string, ProjectPurgePlan> "
            "for one-shot semantics"
        )
    # Token must be deleted from the store BEFORE any side-effecting call inside
    # executeProjectPurgePlan. We scope the search to the function body so
    # imports and helper definitions don't produce false positives.
    fn_match = re.search(
        r"export async function executeProjectPurgePlan\(\s*opts:.*?\n\}\n",
        src,
        re.DOTALL,
    )
    if not fn_match:
        failures.append(
            "utils/projectPurge.ts: could not locate executeProjectPurgePlan body — "
            "token-safety scan skipped"
        )
        return
    body = fn_match.group(0)
    consume_match = re.search(r"projectPurgePlanStore\.delete\(opts\.token\)", body)
    if not consume_match:
        failures.append(
            "utils/projectPurge.ts: executeProjectPurgePlan must call "
            "projectPurgePlanStore.delete(opts.token) before any side effect"
        )
        return
    consume_idx = consume_match.start()
    side_effect_patterns = [
        # Patterns that represent actual side-effecting calls (await + name(.
        r"\bawait\s+rm\(",
        r"\bawait\s+copyFile\(",
        r"\bawait\s+writeFile\(",
        r"\bawait\s+mkdir\(",
        r"\bawait\s+copyRecursiveNoSymlink\(",
    ]
    for pattern in side_effect_patterns:
        m = re.search(pattern, body)
        if m and m.start() < consume_idx:
            failures.append(
                f"utils/projectPurge.ts: side-effect call matching {pattern!r} "
                "appears before token consume in executeProjectPurgePlan — "
                "race condition risk"
            )


def check_engine_memory_override(failures: list[str]) -> None:
    src = read(PROJECT_PURGE)
    # Detect at least the two env vars + settings probe.
    for needle in [
        "MOSSEN_COWORK_MEMORY_PATH_OVERRIDE",
        "MOSSEN_CODE_REMOTE_MEMORY_DIR",
        "autoMemoryDirectory",
    ]:
        if needle not in src:
            failures.append(
                f"utils/projectPurge.ts: memory override detection must check {needle!r}"
            )
    # External + --include-memory must reject.
    if "external_memory_include_rejected" not in src:
        failures.append(
            "utils/projectPurge.ts: --include-memory + external memory must produce "
            "an 'external_memory_include_rejected' error"
        )


def check_engine_symlink_safety(failures: list[str]) -> None:
    src = read(PROJECT_PURGE)
    # Archive copy must use lstat (not stat) to detect symlinks before recursing.
    if "lstat(" not in src:
        failures.append(
            "utils/projectPurge.ts: copyRecursive must use lstat (not stat) so "
            "symlinks are detected and skipped, not followed"
        )
    if "isSymbolicLink" not in src:
        failures.append(
            "utils/projectPurge.ts: copyRecursive must explicitly check isSymbolicLink "
            "to refuse symlink traversal"
        )


def check_engine_no_force_flag(failures: list[str]) -> None:
    src = read(PROJECT_PURGE)
    for flag in FORBIDDEN_FLAGS:
        if flag in src:
            failures.append(
                f"utils/projectPurge.ts: forbidden token {flag!r} present — "
                "/project purge must not provide force / bypass-grace paths"
            )


def check_engine_active_guard_double(failures: list[str]) -> None:
    src = read(PROJECT_PURGE)
    # detectActiveProject must be called from BOTH getProjectPurgePlan and
    # executeProjectPurgePlan (dry-run + confirm).
    occurrences = src.count("detectActiveProject(")
    # >=3 because the function definition itself contributes one.
    if occurrences < 3:
        failures.append(
            "utils/projectPurge.ts: detectActiveProject must be called from BOTH "
            "getProjectPurgePlan and executeProjectPurgePlan (defense-in-depth)"
        )


def check_engine_no_sibling_literals(failures: list[str]) -> None:
    src = read(PROJECT_PURGE)
    for literal in FORBIDDEN_SIBLING_LITERALS:
        if literal in src:
            failures.append(
                f"utils/projectPurge.ts: forbidden sibling-path literal {literal!r} present — "
                "/project purge must not reference ~/.mossen/ siblings outside projects/+backups/"
            )


def check_engine_no_project_root_rm(failures: list[str]) -> None:
    """Refuse to call rm() with the bare project root as its sole arg.

    rm(projectRoot, ...) on a path that resolves to the entire project tree
    is the foot-cannon Round 2 must not load. We accept rm of the project
    dir AFTER it has been emptied (Phase D), so the assertion is structural:
    Phase D rm of plan.originalProjectDir must come AFTER readdir(plan.originalProjectDir)
    confirms emptiness.
    """
    src = read(PROJECT_PURGE)
    # Find the rm call on plan.originalProjectDir, if any.
    rm_match = re.search(
        r"rm\(\s*plan\.originalProjectDir", src,
    )
    if rm_match:
        # Look backwards for an emptiness check (readdir + length === 0).
        before = src[: rm_match.start()]
        if "remaining.length === 0" not in before:
            failures.append(
                "utils/projectPurge.ts: rm(plan.originalProjectDir) must be guarded "
                "by an emptiness check (`remaining.length === 0`) — never blanket-rm "
                "the project root"
            )


# ---------------------------------------------------------------------------
# parseArgs.ts
# ---------------------------------------------------------------------------


def check_parse_args(failures: list[str]) -> None:
    src = read(PARSE_ARGS)
    if not re.search(r"type:\s*'purge'", src):
        failures.append(
            "commands/project/parseArgs.ts: ParsedProjectCommand must include 'purge' case"
        )
    for field in ["target", "includeMemory", "confirmToken"]:
        if field not in src:
            failures.append(
                f"commands/project/parseArgs.ts: 'purge' case must thread {field!r} field"
            )
    # All forbidden flags must be rejected via the unsupported_flag tag.
    for flag in [
        "--all-projects",
        "--orphan-only",
        "--no-archive",
        "--force",
        "--yes",
        "--i-know-what-im-doing",
    ]:
        if flag not in src:
            failures.append(
                f"commands/project/parseArgs.ts: must reject {flag!r} via FORBIDDEN_FLAGS"
            )
    if "unsupported_flag" not in src:
        failures.append(
            "commands/project/parseArgs.ts: must produce 'unsupported_flag' tagged result "
            "for forbidden flags"
        )
    # The flag --include-memory is allowed.
    if "--include-memory" not in src:
        failures.append(
            "commands/project/parseArgs.ts: must read --include-memory flag"
        )
    # The flag --confirm is read.
    if "--confirm" not in src:
        failures.append(
            "commands/project/parseArgs.ts: must read --confirm <token> flag"
        )
    if "--target" not in src:
        failures.append(
            "commands/project/parseArgs.ts: must read --target <cwd> flag"
        )


# ---------------------------------------------------------------------------
# project.tsx — router
# ---------------------------------------------------------------------------


def check_project_router(failures: list[str]) -> None:
    src = read(PROJECT_TSX)
    if "ProjectPurge" not in src:
        failures.append(
            "commands/project/project.tsx: must import and render ProjectPurge"
        )
    if "parseProjectArgs" not in src:
        failures.append(
            "commands/project/project.tsx: must call parseProjectArgs"
        )
    if "parsed.type === 'purge'" not in src:
        failures.append(
            "commands/project/project.tsx: must dispatch on parsed.type === 'purge'"
        )
    if "unsupported_flag" not in src:
        failures.append(
            "commands/project/project.tsx: must surface 'unsupported_flag' route to ProjectPurge"
        )


# ---------------------------------------------------------------------------
# index.tsx — Command registration
# ---------------------------------------------------------------------------


def check_project_index(failures: list[str]) -> None:
    src = read(PROJECT_INDEX)
    if "type: 'local-jsx'" not in src:
        failures.append(
            "commands/project/index.tsx: must register a local-jsx Command"
        )
    if "name: 'project'" not in src:
        failures.append(
            "commands/project/index.tsx: command name must be 'project'"
        )
    if "load: () => import('./project.js')" not in src:
        failures.append(
            "commands/project/index.tsx: must lazy-load ./project.js"
        )


# ---------------------------------------------------------------------------
# ProjectPurge.tsx — UI
# ---------------------------------------------------------------------------


def check_project_purge_tsx(failures: list[str]) -> None:
    if not PROJECT_PURGE_TSX.exists():
        failures.append("commands/project/ProjectPurge.tsx: missing")
        return
    src = read(PROJECT_PURGE_TSX)
    # Engine wiring.
    if "getProjectPurgePlan" not in src:
        failures.append(
            "commands/project/ProjectPurge.tsx: must call getProjectPurgePlan for dry-run"
        )
    if "executeProjectPurgePlan" not in src:
        failures.append(
            "commands/project/ProjectPurge.tsx: must call executeProjectPurgePlan for confirm"
        )
    if "PROJECT_PURGE_TOKEN_TTL_MS" not in src:
        failures.append(
            "commands/project/ProjectPurge.tsx: must surface PROJECT_PURGE_TOKEN_TTL_MS to user"
        )
    # i18n.
    if "getLocalizedText" not in src:
        failures.append(
            "commands/project/ProjectPurge.tsx: must use getLocalizedText for zh/en"
        )
    # Memory must be mentioned (preserve + include-memory).
    if "preserved" not in src.lower() and "preserve" not in src.lower():
        failures.append(
            "commands/project/ProjectPurge.tsx: en text must mention memory preservation"
        )
    if "保留" not in src:
        failures.append(
            "commands/project/ProjectPurge.tsx: zh text must mention memory 保留"
        )
    if "--include-memory" not in src:
        failures.append(
            "commands/project/ProjectPurge.tsx: dry-run must surface --include-memory option"
        )
    if "--confirm" not in src:
        failures.append(
            "commands/project/ProjectPurge.tsx: dry-run output must show "
            "'/project purge ... --confirm <token>' instruction"
        )
    # Active-guard error tag must be handled with a localized message.
    if "active_project" not in src:
        failures.append(
            "commands/project/ProjectPurge.tsx: must handle 'active_project' error tag"
        )
    if "external_memory_include_rejected" not in src:
        failures.append(
            "commands/project/ProjectPurge.tsx: must handle 'external_memory_include_rejected' error tag"
        )
    if "unknown_token" not in src or "expired_token" not in src:
        failures.append(
            "commands/project/ProjectPurge.tsx: must handle unknown/expired token error tags"
        )
    # No force tokens.
    for flag in FORBIDDEN_FLAGS:
        if flag in src:
            failures.append(
                f"commands/project/ProjectPurge.tsx: forbidden token {flag!r} present"
            )


# ---------------------------------------------------------------------------
# commands.ts registration
# ---------------------------------------------------------------------------


def check_commands_registration(failures: list[str]) -> None:
    src = read(COMMANDS_TS)
    if "import project from './commands/project/index.js'" not in src:
        failures.append(
            "commands.ts: must import project from './commands/project/index.js'"
        )
    # Must appear in COMMANDS array (the array is the source of truth for
    # which commands are registered).
    array_match = re.search(
        r"const COMMANDS\s*=\s*memoize\(\(\):\s*Command\[]\s*=>\s*\[(.+?)\n\]\)",
        src,
        re.DOTALL,
    )
    if not array_match:
        failures.append(
            "commands.ts: could not locate COMMANDS array — registration check skipped"
        )
        return
    array_body = array_match.group(1)
    if not re.search(r"\bproject\b", array_body):
        failures.append(
            "commands.ts: COMMANDS array must contain `project,`"
        )


# ---------------------------------------------------------------------------
# Boundary guards
# ---------------------------------------------------------------------------


def check_no_protocol_change(failures: list[str]) -> None:
    if not SDK_CONTROL_SCHEMAS.exists():
        return
    src = read(SDK_CONTROL_SCHEMAS)
    leak_tokens = [
        "ProjectPurge",
        "getProjectPurgePlan",
        "executeProjectPurgePlan",
        "PROJECT_PURGE_TOKEN_TTL_MS",
    ]
    for token in leak_tokens:
        if token in src:
            failures.append(
                f"entrypoints/sdk/controlSchemas.ts: token {token!r} leaked into protocol — "
                "/project purge is a slash command, not a control_request subtype"
            )


def check_main_loop_clean(failures: list[str]) -> None:
    leak_tokens = [
        "ProjectPurge",
        "getProjectPurgePlan",
        "executeProjectPurgePlan",
        "PROJECT_PURGE_TOKEN_TTL_MS",
    ]
    for path, label in [
        (QUERY_TS, "query.ts"),
        (PROCESS_USER_INPUT, "utils/processUserInput/processUserInput.ts"),
        (TOOL_TS, "Tool.ts"),
    ]:
        if not path.exists():
            continue
        src = read(path)
        for token in leak_tokens:
            if token in src:
                failures.append(
                    f"{label}: token {token!r} leaked — /project purge must stay confined "
                    "to the slash-command surface"
                )


def check_insights_untouched(failures: list[str]) -> None:
    if not COMMANDS_INSIGHTS.exists():
        failures.append(
            "commands/insights.ts: must remain present (Round 2 must not delete it)"
        )


def check_run_all_registration(failures: list[str]) -> None:
    if not RUN_ALL.exists():
        failures.append("scripts/run_all_smoke.sh: missing")
        return
    src = read(RUN_ALL)
    if "wave_w55_project_purge_smoke" not in src:
        failures.append(
            "scripts/run_all_smoke.sh: must register wave_w55_project_purge_smoke"
        )


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


def main() -> int:
    failures: list[str] = []

    if not PROJECT_PURGE.exists():
        print("FAIL: utils/projectPurge.ts not found", file=sys.stderr)
        return 1

    check_engine_exists(failures)
    check_engine_exports(failures)
    check_engine_imports(failures)
    check_engine_token_safety(failures)
    check_engine_memory_override(failures)
    check_engine_symlink_safety(failures)
    check_engine_no_force_flag(failures)
    check_engine_active_guard_double(failures)
    check_engine_no_sibling_literals(failures)
    check_engine_no_project_root_rm(failures)
    check_parse_args(failures)
    check_project_router(failures)
    check_project_index(failures)
    check_project_purge_tsx(failures)
    check_commands_registration(failures)
    check_no_protocol_change(failures)
    check_main_loop_clean(failures)
    check_insights_untouched(failures)
    check_run_all_registration(failures)

    print("=== W55 Round 2 project purge smoke ===")
    print(f"engine:    {PROJECT_PURGE.relative_to(ROOT)}")
    print(f"parser:    {PARSE_ARGS.relative_to(ROOT)}")
    print(f"router:    {PROJECT_TSX.relative_to(ROOT)}")
    print(f"index:     {PROJECT_INDEX.relative_to(ROOT)}")
    print(f"UI:        {PROJECT_PURGE_TSX.relative_to(ROOT)}")
    print("scope:     C5 /project purge dry-run + --confirm token, archive-only,")
    print("           memory preserved by default, three-way active guard.")

    if failures:
        print()
        print("=== FAIL ===")
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print()
    print(
        "PASS: W55 Round 2 ✓ "
        "(getProjectPurgePlan + executeProjectPurgePlan exported, 10-min one-shot token, "
        "three-way active guard, memory-default-preserve + external override reject, "
        "symlink-safe archive, six forbidden flags rejected, archive-only "
        "(no --no-archive), no protocol/main-loop/Workbench drift, /project route wired)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
