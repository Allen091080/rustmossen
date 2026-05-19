#!/usr/bin/env bun
/* eslint-disable no-console -- test harness: stdout/stderr is the protocol with the python smoke caller. */
/**
 * W52 Named Plan Files runtime case check.
 *
 * Invoked by `wave_w52_named_plan_files_smoke.py` to exercise
 * `generatePromptPlanSlug` against a fixed set of cases. Static grep
 * inside the python smoke covers contract anchors; this script covers
 * runtime behavior.
 *
 * Exits 0 on full PASS, 1 on any FAIL with a per-case diagnostic on
 * stderr.
 *
 * Pure read; no IO besides stdout/stderr; no session/global state.
 */

import { generatePromptPlanSlug } from '../utils/plans.js'

type Case = {
  label: string
  input: string
  want: string | null
}

const cases: Case[] = [
  { label: 'plain english',          input: 'Refactor auth login flow',                                want: 'refactor-auth-login-flow' },
  { label: 'punctuation noise',      input: 'Fix: API/client timeout!!!',                              want: 'fix-api-client-timeout' },
  { label: 'markdown decorations',   input: '## **Refactor** auth `login` flow ~~ok~~',               want: 'refactor-auth-login-flow-ok' },
  { label: 'path traversal chars',   input: '../../etc/passwd',                                       want: 'etc-passwd' },
  { label: 'ANSI escape stripped',   input: '\x1b[31mError\x1b[0m fix the bug',                       want: 'error-fix-the-bug' },
  { label: 'overflow truncates',     input: 'a'.repeat(200),                                          want: 'a'.repeat(48) },
  { label: 'no trailing dash',       input: 'add new feature -- ',                                    want: 'add-new-feature' },
  { label: 'multiple spaces collapse', input: 'add    extra   spaces',                                want: 'add-extra-spaces' },
  { label: 'empty string',           input: '',                                                       want: null },
  { label: 'whitespace only',        input: '   \t\n  ',                                              want: null },
  { label: 'punctuation only',       input: '!!!???***',                                              want: null },
  { label: 'pure CJK fallback',      input: '重构认证模块',                                              want: null },
  { label: 'emoji only fallback',    input: '🔥🔥🔥',                                                  want: null },
  { label: 'CJK dominated fallback', input: '修复 bug 一下',                                            want: null }, // 4 CJK + 3 ASCII = 3/7 < 0.5 bias floor
  { label: 'too short fallback',     input: 'a',                                                      want: null }, // 1 char after collapse < min length 2
]

let failures = 0
let passed = 0

for (const c of cases) {
  const got = generatePromptPlanSlug(c.input)
  const ok = got === c.want
  if (ok) {
    passed++
    console.log(`OK   [${c.label}] -> ${JSON.stringify(got)}`)
  } else {
    failures++
    console.error(
      `FAIL [${c.label}] input=${JSON.stringify(c.input.slice(0, 60))} ` +
        `want=${JSON.stringify(c.want)} got=${JSON.stringify(got)}`,
    )
  }
}

console.log('')
console.log(`=== W52 runtime cases: ${passed} pass / ${failures} fail / ${cases.length} total ===`)
process.exit(failures > 0 ? 1 : 0)
