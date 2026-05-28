#!/usr/bin/env python3
"""
i18n_runtime_smoke — UX-Wave1 S5 产物。

补齐 W1 i18n 工程的端到端静态 smoke 维度，作为 i18n_self_check.py 的延伸。

两者关系：
  i18n_self_check.py     → 字典内部约束（en/zh 对称、三级命名、无空值）
  i18n_runtime_smoke.py  → 字典与代码集成约束（placeholder 一致、
                           迁移点存在、关键文件已 import t()、
                           hosted 入口 isCustomBackendEnabled() gate）

校验维度：
  M1. en/zh 同 key 的 {placeholder} 集合一致（zh 漏 placeholder 会让
      runtime 渲染留下字面量 {product}，影响中文用户体验）
  M2. S2A/S3/S3 续/S4A/S4B/S4C 迁移涉及的关键 key 全部存在于字典
      （防 commit 后误删）
  M3. 关键源文件 import 了 t (或 hasI18nKey / i18nT alias)
      （防 commit 后误删 import）
  M4. S9-impl 6 个 hosted 入口 cmd index 都含 isCustomBackendEnabled()
      字面量（防 commit 后被回滚露出 hosted 入口）

不做：
  - 不启动 mossen runtime；不动 ~/.mossen 用户配置（红线）
  - 不替代 i18n_self_check.py（跑这个脚本前先跑那个）
  - 不替代 W1.4/W1.5 实机 smoke 清单（语言切换实际渲染要人工看）

Exit code:
  0  — 4 维度全部通过
  1  — 任一维度失败；stderr 列具体失败项
  2  — 参数错误 / IO 异常

Usage:
  python3 scripts/i18n_runtime_smoke.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EN_PATH = REPO_ROOT / "utils" / "i18n" / "strings.en.ts"
ZH_PATH = REPO_ROOT / "utils" / "i18n" / "strings.zh.ts"

# 同时匹配单/双引号包裹的 key 和 value (含中英文 / placeholder / 长串)
KV_LINE = re.compile(
    r"""^\s*['"]([a-z][a-z0-9-]*\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)['"]\s*:\s*"""
    r"""(?:'([^'\\]*(?:\\.[^'\\]*)*)'|"([^"\\]*(?:\\.[^"\\]*)*)")""",
    re.MULTILINE,
)
PLACEHOLDER = re.compile(r"\{(\w+)\}")

# ---- M2: W1 各 slice 期望的迁移 key 清单 ----
EXPECTED_KEYS_BY_SLICE: dict[str, list[str]] = {
    "S1": ["ui.welcome.title"],
    "S2A": [
        "cmd.help.description",
        "cmd.exit.description",
        "cmd.files.description",
        "cmd.memory.description",
        "cmd.mcp.description",
        "cmd.skills.description",
        "cmd.hooks.description",
        "cmd.resume.description",
        "cmd.lang.description",
    ],
    "S3": [
        "ui.taskSummary.tasks",
        "ui.taskSummary.done",
        "ui.taskSummary.inProgress",
        "ui.taskSummary.open",
        "ui.taskSummary.pending",
        "ui.taskSummary.completed",
        "ui.task.blockedByLabel",
    ],
    "S3-cont": [
        "ui.taskActivity.stopping",
        "ui.taskActivity.awaitingApproval",
        "ui.taskActivity.idle",
        "ui.taskActivity.working",
    ],
    "S4A": [
        "lang.cleared.message",
        "lang.current.label",
        "lang.preference.label",
        "lang.preference.auto",
        "lang.usage.line",
        "lang.usage.shortcut",
        "lang.usage.note",
        "lang.switched.message",
    ],
    "S4B": [
        "ui.exit.goodbye1",
        "ui.exit.goodbye2",
        "ui.exit.goodbye3",
        "ui.exit.goodbye4",
        "ui.interrupted.label",
        "ui.interrupted.hint",
    ],
    "S4C": [
        "ui.compact.summarizedTitle",
        "ui.compact.summarizedDetailUpTo",
        "ui.compact.summarizedDetailFrom",
        "ui.compact.contextLabel",
        "ui.compact.summaryTitle",
        "ui.compact.expandHistoryHint",
        "ui.compact.expandHint",
    ],
    # W2-S1: 高频会话基础 10 命令（9 A + 1 B with {product} placeholder）
    "W2-S1": [
        "cmd.clear.description",
        "cmd.compact.description",
        "cmd.diff.description",
        "cmd.copy.description",
        "cmd.export.description",
        "cmd.branch.description",
        "cmd.rename.description",
        "cmd.tasks.description",
        "cmd.usage.description",
        "cmd.rewind.description",
    ],
    # W2-S2: 编辑 / 配置 8 命令（全 A）
    "W2-S2": [
        "cmd.config.description",
        "cmd.theme.description",
        "cmd.color.description",
        "cmd.keybindings.description",
        "cmd.vim.description",
        "cmd.effort.description",
        "cmd.profile.description",
        "cmd.plan.description",
    ],
    # W2-S3: PR / Review / 安全 / 登录 / 顾问 4 命令（3 A + 1 B；/review 暂缓）
    "W2-S3": [
        "cmd.advisor.description",
        "cmd.security-review.description",
        "cmd.permissions.description",
        "cmd.login.description",
    ],
    # W2-S4: Plugin / Skill / IDE 5 命令（全 A；/plugin 暂缓）
    "W2-S4": [
        "cmd.reload-plugins.description",
        "cmd.agents.description",
        "cmd.ide.description",
        "cmd.init-verifiers.description",
        "cmd.add-dir.description",
    ],
    # W2-S5: 系统/杂项 1 命令（/context 推迟 multi-variant；/brief→D；/logout→D）
    "W2-S5": [
        "cmd.btw.description",
    ],
}

# ---- M3: 关键源文件 import 守卫 (file → 必须含的子串之一) ----
# 任一子串命中即可（应对 t / i18nT 等 alias 形式）
IMPORT_GUARD: dict[str, list[str]] = {
    "utils/commandDescription.ts": ["from './i18n/index.js'", "hasI18nKey"],
    "components/TaskListV2.tsx": ["from '../utils/i18n/index.js'"],
    "components/tasks/taskStatusUtils.tsx": ["from 'src/utils/i18n/index.js'"],
    "commands/lang/lang.tsx": ["from '../../utils/i18n/index.js'"],
    "components/ExitFlow.tsx": ["from '../utils/i18n/index.js'"],
    "commands/exit/exit.tsx": ["from '../../utils/i18n/index.js'"],
    "components/InterruptedByUser.tsx": ["from '../utils/i18n/index.js'"],
    "components/CompactSummary.tsx": ["from '../utils/i18n/index.js'"],
}

# ---- M4: S9-impl 6 个 hosted 入口必须含的 gate 字面量 ----
HOSTED_GATE_FILES = [
    "commands/chrome/index.ts",
    "commands/remote-setup/index.ts",
    "commands/upgrade/index.ts",
    "commands/desktop/index.ts",
    "commands/install-github-app/index.ts",
    "commands/fast/index.ts",
]
HOSTED_GATE_LITERAL = "isCustomBackendEnabled()"


def parse_strings_dict(path: Path) -> dict[str, str]:
    if not path.exists():
        print(f"[i18n_runtime_smoke] missing file: {path}", file=sys.stderr)
        sys.exit(2)
    text = path.read_text(encoding="utf-8")
    result: dict[str, str] = {}
    for m in KV_LINE.finditer(text):
        key = m.group(1)
        value = m.group(2) if m.group(2) is not None else m.group(3) or ""
        result[key] = value
    return result


def check_m1_placeholder_consistency(
    en: dict[str, str], zh: dict[str, str]
) -> list[str]:
    failures: list[str] = []
    for key, en_val in en.items():
        if key not in zh:
            continue  # 由 i18n_self_check.py 报对称性失败，本脚本不重复
        en_phs = set(PLACEHOLDER.findall(en_val))
        zh_phs = set(PLACEHOLDER.findall(zh[key]))
        if en_phs != zh_phs:
            only_en = en_phs - zh_phs
            only_zh = zh_phs - en_phs
            parts: list[str] = []
            if only_en:
                parts.append(f"only-in-en={sorted(only_en)}")
            if only_zh:
                parts.append(f"only-in-zh={sorted(only_zh)}")
            failures.append(f"{key}: " + "; ".join(parts))
    return failures


def check_m2_migrated_keys_present(en_keys: set[str]) -> list[str]:
    missing: list[str] = []
    for slice_id, keys in EXPECTED_KEYS_BY_SLICE.items():
        for key in keys:
            if key not in en_keys:
                missing.append(f"[{slice_id}] {key}")
    return missing


def check_m3_imports() -> list[str]:
    failures: list[str] = []
    for rel_path, hints in IMPORT_GUARD.items():
        path = REPO_ROOT / rel_path
        if not path.exists():
            failures.append(f"missing file: {rel_path}")
            continue
        text = path.read_text(encoding="utf-8")
        if not any(hint in text for hint in hints):
            failures.append(
                f"{rel_path}: none of {hints} found (lost t() import?)"
            )
    return failures


def check_m4_hosted_gates() -> list[str]:
    failures: list[str] = []
    for rel_path in HOSTED_GATE_FILES:
        path = REPO_ROOT / rel_path
        if not path.exists():
            failures.append(f"missing file: {rel_path}")
            continue
        text = path.read_text(encoding="utf-8")
        if HOSTED_GATE_LITERAL not in text:
            failures.append(
                f"{rel_path}: missing '{HOSTED_GATE_LITERAL}' "
                f"(S9-impl gate regression?)"
            )
    return failures


def main() -> int:
    en = parse_strings_dict(EN_PATH)
    zh = parse_strings_dict(ZH_PATH)
    en_keys = set(en.keys())

    sections: list[tuple[str, list[str]]] = [
        ("M1 placeholder consistency", check_m1_placeholder_consistency(en, zh)),
        ("M2 migrated keys present", check_m2_migrated_keys_present(en_keys)),
        ("M3 source file t() imports", check_m3_imports()),
        ("M4 hosted-gate isCustomBackendEnabled() literals", check_m4_hosted_gates()),
    ]

    any_fail = False
    for label, failures in sections:
        if failures:
            any_fail = True
            print(f"[i18n_runtime_smoke] FAIL — {label}:", file=sys.stderr)
            for f in failures:
                print(f"  - {f}", file=sys.stderr)
        else:
            print(f"[i18n_runtime_smoke] OK   — {label}")

    if any_fail:
        return 1

    total_expected = sum(len(v) for v in EXPECTED_KEYS_BY_SLICE.values())
    print(
        f"\n[i18n_runtime_smoke] SUMMARY: {len(en_keys)} keys total; "
        f"{total_expected} W1-migrated keys present; "
        f"{len(IMPORT_GUARD)} files import t(); "
        f"{len(HOSTED_GATE_FILES)} hosted gates intact."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
