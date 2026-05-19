# W57 — Second-tier closure (C1 + A1 + A2 + C7 + D6)

**Status**: implemented + smoke green (C1 / A1 / A2); deferred (C7 / D6)
**Date**: 2026-05-03

## Scope

Five second-tier subitems carried over from the upgrade backlog. Three
land as code (C1 picker, A1/A2 baselines). Two are deferred with reasons
captured here so the next wave can pick them up cleanly.

| ID  | Subject                                | Decision     | Files                                   |
| --- | -------------------------------------- | ------------ | --------------------------------------- |
| C1  | `/effort` interactive picker UI        | IMPLEMENTED  | `commands/effort/EffortPicker.tsx` (NEW), `commands/effort/effort.tsx` (modified) |
| A1  | PromptInput perf baseline (read-only)  | IMPLEMENTED  | `scripts/wave_w57_promptinput_baseline.py` (NEW) |
| A2  | `/resume` token-cost baseline (read-only) | IMPLEMENTED | `scripts/wave_w57_resume_token_baseline.py` (NEW) |
| C7  | Vim visual mode in `useVimInput.ts`    | **DEFERRED** | (see "C7 defer rationale" below)        |
| D6  | Local audit-log writer                 | **DEFERRED** | (see "D6 defer rationale" below) — no writer module, no caller, no env var, no smoke ships in this wave |

## C1 — `/effort` interactive picker

### Behaviour change

| Args                              | Before               | After                |
| --------------------------------- | -------------------- | -------------------- |
| (empty)                           | shows current effort | **opens picker**     |
| `current` / `status`              | shows current effort | shows current effort |
| `low` / `medium` / `high` / `max` | applies directly     | applies directly     |
| `auto` / `unset`                  | clears override      | clears override      |
| `help` / `-h` / `--help`          | prints usage         | prints usage         |

### Why this routing

The picker is the only path that renders a UI. Every legacy subcommand
still works exactly as before — `executeEffort(args)` is the single
write surface, used by both the picker and the direct-arg path, so no
divergence is possible. **No new configuration write path was added.**

### Picker chrome

- `components/design-system/Dialog.js` for the modal frame (matches W56
  patterns landed for `/project list` and `/project status`).
- `components/CustomSelect/select.js` for the option list with inline
  descriptions.
- Order: `auto`, `low`, `medium`, `high`, `max`.
- `max` is rendered with a `(Opus 4.6 only)` suffix and `disabled: true`
  when `modelSupportsMaxEffort(model)` returns false. (Disabled rather
  than hidden, so the user sees why it isn't selectable.)
- Cancel (Esc) emits a localized "no change applied" message; localized
  pair (en / zh) is asserted by the smoke.

### Smoke
`scripts/wave_w57_effort_picker_smoke.py` — 8 assertions, all PASS.

## A1 — PromptInput perf baseline

### Goal
Measure, do not change. The smoke catalogs the per-keystroke `useMemo`
hot spots in `components/PromptInput/PromptInput.tsx` so any future
optimisation wave can prove the surface area before it touches code.
**No `PromptInput` performance code was modified in this wave.**

### Tracked hot spots (file:line in PromptInput.tsx)

| Helper                          | Source                                            | Recomputes on               |
| ------------------------------- | ------------------------------------------------- | --------------------------- |
| `findThinkingTriggerPositions`  | `utils/thinking.ts`                               | `displayedValue`            |
| `findBtwTriggerPositions`       | `utils/sideQuestion.ts`                           | `displayedValue`            |
| `findBuddyTriggerPositions`     | `buddy/useBuddyNotification.tsx`                  | `displayedValue`            |
| `findSlashCommandPositions`     | `utils/suggestions/commandSuggestions.ts`         | `displayedValue + commands` |
| `findTokenBudgetPositions`      | `utils/tokenBudget.ts`                            | `displayedValue` (gated by `feature('TOKEN_BUDGET')`) |
| `findSlackChannelPositions`     | `utils/suggestions/slackChannelSuggestions.ts`    | `displayedValue + knownChannelsVersion` |
| `parseReferences`               | `history.ts`                                      | `displayedValue`            |

### Static observations

- **PromptInput.tsx**: 2305 lines. Component is React-compiler-output
  and uses 63 hook calls.
- **PromptInput dir**: 21 files, 5207 LOC.
- **Per-call `/g` regex**: 6 of 7 hot-spot helpers compile a fresh `/g`
  RegExp inside the call. Intentional — see the comment in
  `utils/thinking.ts` about `matchAll` `lastIndex` leakage. Recorded so
  a future "cache the regex" optimisation must audit the state-leak
  risk explicitly.
- **`displayedValue`** is itself memoised — that selector is the choke
  point. Making it cheaper would benefit all 7 helpers at once.

### Worth optimising next?

**Probably not yet.** All 7 helpers are O(N) over `displayedValue`. For
typical prompt lengths (under 5K chars) the per-keystroke cost is
microseconds; only paste paths above ~10K chars are likely to matter,
and the existing paste handler in `inputPaste.ts` already takes a
non-incremental shape that bypasses many of these helpers. A future
wave should produce a real perf harness (Bun benchmark or React
profiler) and compare 1k / 5k / 10k character inputs before deciding
which helper to fold or memoise harder.

### Smoke
`scripts/wave_w57_promptinput_baseline.py` — fails if any tracked
helper disappears, moves out of `useMemo`, or the `displayedValue`
selector loses its memoisation.

## A2 — `/resume` token-cost baseline

### What "token efficiency" actually means here

The picker itself does **not** emit tokens. Selection enumeration uses
lite-only metadata reads (a 64 KiB tail window per session via
`LITE_READ_BUF_SIZE` in `utils/sessionStoragePortable.ts`). The full
transcript is loaded (via `loadFullLog`) only **after** the user picks
a row, types a UUID, or types an exact custom title — and at that
point the cost is fundamentally proportional to the resumed session.

**No resume / sessionStorage code was modified in this wave.**

### Invariants the smoke locks in

1. `LITE_READ_BUF_SIZE = 65536` (64 KiB tail per session). Any change
   forces a baseline refresh.
2. `loadFullLog` is called **only after** a lite loader (positional
   ordering check inside `commands/resume/resume.tsx`).
3. All three `loadFullLog` call sites are gated by
   `isLiteLog(log) ? await loadFullLog(log) : log`. A non-lite log is
   never re-loaded.
4. `ResumeCommand` body never calls `loadFullLog` before `handleSelect`
   (eager-load regression guard).
5. Both progressive lite loaders
   (`loadAllProjectsMessageLogsProgressive`,
   `loadSameRepoMessageLogsProgressive`) remain present.

### Worth optimising next?

**Not from a token angle.** The picker is already lite-only. A future
wave that wants `/resume` to feel snappier should instead measure
- index file build time (`getSessionFilesLite`)
- the `enrichLogs` per-session 64 KiB read latency on cold disk
- progressive-load batch sizes

…none of which are token costs. The token cost lives entirely in the
resumed session's transcript size, which the user already paid for.

### Smoke
`scripts/wave_w57_resume_token_baseline.py` — 7 assertions, all PASS.

## C7 — Vim visual mode (DEFERRED)

### What was investigated

`hooks/useVimInput.ts` (316 lines) currently exposes
`VimMode = 'INSERT' | 'NORMAL'` (`types/textInputTypes.ts:222`). The
state machine in `vim/types.ts` mirrors that with
`VimState = { mode: 'INSERT' | 'NORMAL'; … }`. Adding a third mode
would require:

1. Extending the `VimMode` type union.
2. Extending `VimState` discriminated union with selection
   `{ start: number; end: number }`.
3. Handling the `v` / `V` keys in the NORMAL → VISUAL transition and
   in the operator pipeline (`d`, `c`, `y`, `~`, etc.).
4. **Rendering the selection** — and this is where it stops being a
   `useVimInput.ts`-only change.

### Defer rationale

The selection highlight has to be **rendered** somewhere. The only
sink for `TextHighlight[]` in the prompt UI is the
`combinedHighlights` `useMemo` in
`components/PromptInput/PromptInput.tsx:592`. There is no clean
injection point for a hook-owned highlight; the hook would have to
either (a) take a callback prop and have PromptInput merge the
selection range into `combinedHighlights`, or (b) own its own
highlight reducer that PromptInput consumes. Both shapes require
modifying PromptInput.tsx (2305 lines, React-compiler output) — which
the W57 spec explicitly defers.

The footer indicator (`PromptInputFooter.tsx`) already takes
`vimMode: VimMode | undefined`, so labelling a third mode is trivial,
but the footer is the cosmetic half. Without selection rendering the
mode is functionally invisible and operators (`vw d` etc.) silently
delete without preview — strictly worse UX than today.

### Re-open conditions

C7 is re-enabled when one of the following is true:
- A future wave already has a sanctioned reason to touch
  `combinedHighlights` (e.g., a selection-rendering refactor for
  another feature) and can absorb the visual-mode hook there.
- A `useTextInputHighlights` hook lands that owns the highlight
  reducer outside PromptInput (would also unlock features like
  search-result highlighting).

## D6 — Local audit-log writer (DEFERRED)

### Decision (this wave)

**No code lands.** No writer module, no caller, no env var, no JSONL
on disk, no smoke that asserts a writer exists. The consolidated W57
smoke asserts `utils/auditLog.ts` is **absent** and that no D6
identifiers (`writeAuditRecord`, `AuditEventKind`, `AuditRecord`,
`isAuditLogEnabled`, `getAuditLogDir`, `getAuditLogFileForDate`,
`MOSSEN_CODE_AUDIT_LOG`) appear anywhere in the codebase.

### Defer rationale

A writer module without a redaction layer and without a viewer is a
liability waiting to happen: the moment any caller starts logging
`details`, we've created a new local PII surface with no review and
no pruning policy. Even a "writer-only, no caller" deliverable invites
silent integration in a later wave that doesn't realise the
prerequisites weren't met.

### Prerequisites for the next wave (D6-A)

D6 only proceeds when **all five** of the following are designed,
reviewed, and have at least one smoke each:

1. **Redaction layer** — pure function that wraps the serializer and,
   at minimum: strips path tails for filesystem events, redacts
   `apiKey` / `authorization` / `token` keys (and their snake_case
   variants) from `details`, and truncates strings past a documented
   ceiling. Must have a test fixture covering each redaction rule.
2. **Event schema** — closed `AuditEventKind` enum, frozen
   `AuditRecord` shape, versioning strategy (`schemaVersion: 1`), and
   a written rule for adding new kinds in future waves.
3. **Caller approval list** — the *exact* file:line locations that
   may call the writer, listed in this doc, with a rationale for each.
   Drift from the list fails the smoke. (Recommended starting point:
   `utils/permissions/permissions.ts`'s three
   `logEvent('tengu_auto_mode_decision', …)` sites at lines 627 / 667
   / 734 — but only after redaction lands.)
4. **Rotation / size limits** — a documented retention policy:
   maximum file size before rotation, maximum days retained, behaviour
   when the disk is near full. The smoke asserts the policy is
   implemented (e.g., a `pruneOldAuditLogs()` helper exists and is
   wired into a known startup path).
5. **Viewer / read path** — a way for the user to inspect the log
   without `cat`. Either a `/audit` command, or `mossen --audit-log`
   flag, or documented "read this file with $TOOL". Whichever shape
   wins, it ships with D6-A — not after.

### Until D6-A

- `utils/auditLog.ts` MUST NOT exist.
- No identifier from the D6 surface (`writeAuditRecord` etc.) may
  appear anywhere in `*.ts` / `*.tsx` / `*.js` / `*.py`.
- No `appendFile` / `createWriteStream` may be added under
  `utils/audit*`.
- No `MOSSEN_CODE_AUDIT_LOG` env var read.
- The W57 consolidated smoke enforces all of the above.

## Validation

```
$ python3 scripts/wave_w57_second_tier_closure_smoke.py
[PASS] W57 — implemented C1/A1/A2 + deferred C7/D6 (D6 writer absent, no callers)
```

- `bun run typecheck:diff` — 0 new errors
- `bun run lint:diff`      — 0 new
- `python3 scripts/wave_w57_effort_picker_smoke.py`     — PASS
- `python3 scripts/wave_w57_promptinput_baseline.py`    — PASS
- `python3 scripts/wave_w57_resume_token_baseline.py`   — PASS

## Files

```
NEW   commands/effort/EffortPicker.tsx                              (133 lines)
EDIT  commands/effort/effort.tsx                                    (router + import)
NEW   scripts/wave_w57_effort_picker_smoke.py
NEW   scripts/wave_w57_promptinput_baseline.py
NEW   scripts/wave_w57_resume_token_baseline.py
NEW   scripts/wave_w57_second_tier_closure_smoke.py
NEW   docs/upgrade/W57-second-tier-closure.md
EDIT  scripts/run_all_smoke.sh                                      (register W57 smoke)
```
