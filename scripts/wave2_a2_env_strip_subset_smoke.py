#!/usr/bin/env python3
"""Wave 2A — A2 (MOS-ENV-STRIP) focused smoke (static-only).

Verifies that MOSSEN_ONLY_SAFE_ENV_VARS in tools/BashTool/bashPermissions.ts
has been C-2 缩 from 30 → 6 OS-level safe env vars, removing the 24 high-risk
or provider-internal env vars that previously could be passed through unchecked
when USER_TYPE === 'mossen'.

Why static-only (not runtime):
  * MOSSEN_ONLY_SAFE_ENV_VARS is module-private (no `export`); cannot be
    introspected via `bun -e` import.
  * tools/BashTool/bashPermissions.ts has deferred runtime imports (REPLTool
    chain) that fail under `bun -e` against source — same constraint as
    Wave 0 PERM-2 / API-001 / Wave 2 A1.
  * The C-2 change is purely set-membership; static membership assertions
    + 4-callsite preservation check are sufficient. Real runtime stripping
    behavior (DOCKER_HOST=evil docker ps NOT stripped) is exercised by the
    full TUI smoke harness — not in scope for this focused smoke.

Allowlist contract — 6 keep / 24 delete:
  Keep:    CI / COLUMNS / TMUX / SKIP_NODE_VERSION_CHECK /
           GIT_LFS_SKIP_SMUDGE / CUDA_VISIBLE_DEVICES
  Delete:  KUBECONFIG / DOCKER_HOST / AWS_PROFILE / PGPASSWORD / GH_TOKEN /
           GROWTHBOOK_API_KEY / CLOUDSDK_CORE_PROJECT / CLUSTER /
           FIRESTORE_EMULATOR_HOST / MONOREPO_ROOT_DIR /
           COO_CLUSTER / COO_CLUSTER_NAME / COO_NAMESPACE /
           COO_LAUNCH_YAML_DRY_RUN / EXPECTTEST_ACCEPT / DBT_PER_DEVELOPER_ENVIRONMENTS /
           STATSIG_FORD_DB_CHECKS / TEST_CROSSCHECK_LISTS_MATCH_UPDATE /
           MOSSEN_ENVIRONMENT / MOSSEN_SERVICE /
           JAX_PLATFORMS / POSTGRESQL_VERSION / HARNESS_QUIET / PYENV_VERSION

Also asserts:
  * 4 callsites of `MOSSEN_ONLY_SAFE_ENV_VARS.has(varName)` preserved
    (we shrink the set, not the gate)

No LLM, no real backend, no ~/.mossen write. Pure file read + regex.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "tools" / "BashTool" / "bashPermissions.ts"

KEEP = {
    "CI",
    "COLUMNS",
    "TMUX",
    "SKIP_NODE_VERSION_CHECK",
    "GIT_LFS_SKIP_SMUDGE",
    "CUDA_VISIBLE_DEVICES",
}

DELETE = {
    # 10 高危 (网络端点重定向)
    "KUBECONFIG",
    "DOCKER_HOST",
    "AWS_PROFILE",
    "PGPASSWORD",
    "GH_TOKEN",
    "GROWTHBOOK_API_KEY",
    "CLOUDSDK_CORE_PROJECT",
    "CLUSTER",
    "FIRESTORE_EMULATOR_HOST",
    "MONOREPO_ROOT_DIR",
    # 10 provider 内部
    "COO_CLUSTER",
    "COO_CLUSTER_NAME",
    "COO_NAMESPACE",
    "COO_LAUNCH_YAML_DRY_RUN",
    "EXPECTTEST_ACCEPT",
    "DBT_PER_DEVELOPER_ENVIRONMENTS",
    "STATSIG_FORD_DB_CHECKS",
    "TEST_CROSSCHECK_LISTS_MATCH_UPDATE",
    "MOSSEN_ENVIRONMENT",
    "MOSSEN_SERVICE",
    # 4 边界版本/调试
    "JAX_PLATFORMS",
    "POSTGRESQL_VERSION",
    "HARNESS_QUIET",
    "PYENV_VERSION",
}


def static_assertion() -> dict[str, object]:
    text = TARGET.read_text(encoding="utf-8")

    findings: dict[str, object] = {
        "set_block_found": False,
        "set_member_count": 0,
        "set_members": [],
        "kept_members_present": [],
        "deleted_members_leaked": [],
        "callsite_count": 0,
    }

    # Locate `const MOSSEN_ONLY_SAFE_ENV_VARS = new Set([...])` block.
    block_re = re.compile(
        r"const MOSSEN_ONLY_SAFE_ENV_VARS = new Set\(\[(?P<body>[\s\S]*?)\]\)",
        re.MULTILINE,
    )
    m = block_re.search(text)
    if m is None:
        return findings
    body = m.group("body")
    findings["set_block_found"] = True

    # Extract quoted string members.
    members = re.findall(r"'([A-Z_][A-Z0-9_]*)'", body)
    members_set = set(members)
    findings["set_member_count"] = len(members)
    findings["set_members"] = sorted(members_set)
    findings["kept_members_present"] = sorted(KEEP & members_set)
    findings["deleted_members_leaked"] = sorted(DELETE & members_set)

    # 4 callsites of `MOSSEN_ONLY_SAFE_ENV_VARS.has(varName)` preserved
    findings["callsite_count"] = len(
        re.findall(r"MOSSEN_ONLY_SAFE_ENV_VARS\.has\(", text)
    )

    return findings


def main() -> int:
    failures: list[str] = []
    findings = static_assertion()

    if not findings["set_block_found"]:
        failures.append(
            "MOSSEN_ONLY_SAFE_ENV_VARS Set 定义块未找到 — A2 不应删除整个 Set"
        )
    else:
        if findings["set_member_count"] != 6:
            failures.append(
                f"Set 成员数 = {findings['set_member_count']},预期 6 (C-2 缩 30→6)"
            )
        if findings["kept_members_present"] != sorted(KEEP):
            missing = sorted(KEEP - set(findings["kept_members_present"]))
            failures.append(
                f"6 项保留成员缺失: {missing} (KEEP set 不完整)"
            )
        if findings["deleted_members_leaked"]:
            failures.append(
                f"24 项删除成员仍残留: {findings['deleted_members_leaked']}"
            )

    if findings["callsite_count"] != 4:
        failures.append(
            f"MOSSEN_ONLY_SAFE_ENV_VARS.has() 调用点 = {findings['callsite_count']},"
            "预期 4 (A2 仅缩集合, 不应改 4 个剥离调用点)"
        )

    report = {
        "name": "wave2_a2_env_strip_subset_smoke",
        "mode": "static-only",
        "mode_reason": (
            "MOSSEN_ONLY_SAFE_ENV_VARS 模块私有 (无 export);bashPermissions.ts "
            "deferred runtime imports 使 `bun -e` 解析失败。C-2 集合缩减 + 4 callsite "
            "保留属纯结构改动,静态断言已足够;真实剥离行为由 TUI smoke 兜底。"
        ),
        "static_findings": findings,
        "failures": failures,
        "passed": 0 if failures else 1,
        "total": 1,
    }
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
