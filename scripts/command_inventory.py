#!/usr/bin/env python3
"""
Mossen slash command inventory 自动生成器。

扫描两条注册路径并输出一份 markdown 表格：
  1. commands/{name}/*.ts(x) 或 commands/{name}.ts(x) —— 传统命令（约 108 条）
  2. skills/bundled/*.ts 里 `userInvocable: true` 的 skill —— skill 命令（约 15 条）

用法：
  python3 scripts/command_inventory.py                        # 打印到 stdout
  python3 scripts/command_inventory.py > docs/command_inventory.md
  python3 scripts/command_inventory.py --json                 # 机器可读 JSON
  python3 scripts/command_inventory.py --count                # 仅统计

设计：
- 静态扫描，不依赖 bun/ts 运行时
- 正则提取（不做完整 TS AST 解析），足以获取 name/description/可见性 gate
- 未能静态提取的字段（"依赖 custom backend / 依赖本地 / 依赖 hosted / 试用结论"）
  留空待人工补填（P1-5 slice B 在用户本机完成）

架构参考：MOSSEN.md §3.4（两条注册路径）、§3.5（七种可见性门）。
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, asdict, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
COMMANDS_DIR = ROOT / "commands"
SKILLS_DIR = ROOT / "skills" / "bundled"

# 简单正则。尽量宽松但不吃到字符串/注释内的噪音。
NAME_RE = re.compile(r"""\bname\s*:\s*['"]([A-Za-z0-9_\-]+)['"]""")
DESC_LITERAL_RE = re.compile(
    r"""\bdescription\s*:\s*['"`]([^'"`\n]{4,300})['"`]""",
    re.DOTALL,
)
DESC_GETTER_RE = re.compile(
    r"""get\s+description\s*\(\s*\)\s*\{[\s\S]{0,500}?return\s+['"`]([^'"`\n]{4,300})['"`]""",
)
ALIASES_RE = re.compile(
    r"""\baliases\s*:\s*\[([^\]]{1,200})\]""",
)
IS_HIDDEN_LITERAL_RE = re.compile(r"""\bisHidden\s*:\s*(true|false)\b""")
IS_HIDDEN_GETTER_RE = re.compile(r"""\bget\s+isHidden\s*\(""")
AVAILABILITY_RE = re.compile(
    r"""\bavailability\s*:\s*\[([^\]]{1,100})\]""",
)
AVAILABILITY_GETTER_RE = re.compile(r"""\bget\s+availability\s*\(""")
IS_ENABLED_RE = re.compile(r"""\bisEnabled\s*:""")
DEFERRED_RE = re.compile(r"""\bisDeferredSlashCommandEnabled\s*\(\s*['"]([A-Za-z0-9_\-]+)['"]""")
FEATURE_RE = re.compile(r"""\bfeature\s*\(\s*['"]([A-Z0-9_]+)['"]""")
GROWTHBOOK_RE = re.compile(r"""\bgetFeatureValue_CACHED[A-Z_]*\s*\(\s*['"]([A-Za-z0-9_]+)['"]""")

# Skills 专用
USER_INVOCABLE_RE = re.compile(r"""\buserInvocable\s*:\s*(true|false)\b""")
REGISTER_SKILL_RE = re.compile(r"""\bregisterBundledSkill\s*\(""")


@dataclass
class CommandEntry:
    name: str
    kind: str  # "command-dir" | "command-file" | "command-stub" | "skill"
    source_path: str
    aliases: list[str] = field(default_factory=list)
    description: str = ""
    is_hidden: str = "?"  # "true" | "false" | "getter" | "?"
    availability: str = ""  # "['hosted']" etc., or "getter"
    has_is_enabled: bool = False
    deferred_gate: str = ""  # e.g. "heapdump" — the name passed to isDeferredSlashCommandEnabled
    feature_gates: list[str] = field(default_factory=list)
    growthbook_flags: list[str] = field(default_factory=list)
    is_stub: bool = False  # 老官方命令被 hard-cut 后保留的 placeholder (isEnabled:()=>false + isHidden:true)
    dir_name: str = ""  # 目录名（如果是 command-dir / command-stub）— 方便后续对齐
    # skill 专用
    user_invocable: str = ""  # "true" | "false" | "" (N/A)


def _text(p: Path) -> str:
    try:
        return p.read_text(errors="ignore")
    except OSError:
        return ""


def _extract_description(text: str) -> str:
    m = DESC_LITERAL_RE.search(text)
    if m:
        return m.group(1).strip()[:160]
    m = DESC_GETTER_RE.search(text)
    if m:
        return m.group(1).strip()[:160]
    return ""


def _extract_aliases(text: str) -> list[str]:
    m = ALIASES_RE.search(text)
    if not m:
        return []
    inside = m.group(1)
    return re.findall(r"""['"]([A-Za-z0-9_\-]+)['"]""", inside)


def _is_hidden_state(text: str) -> str:
    m = IS_HIDDEN_LITERAL_RE.search(text)
    if m:
        return m.group(1)
    if IS_HIDDEN_GETTER_RE.search(text):
        return "getter"
    return "?"


def _availability(text: str) -> str:
    m = AVAILABILITY_RE.search(text)
    if m:
        return "[" + m.group(1).strip() + "]"
    if AVAILABILITY_GETTER_RE.search(text):
        return "getter"
    return ""


def _deferred_gate(text: str, fallback_name: str) -> str:
    m = DEFERRED_RE.search(text)
    if m:
        return m.group(1)
    return ""


def _list_unique(pattern: re.Pattern, text: str) -> list[str]:
    return sorted(set(pattern.findall(text)))


def _last_name_match(text: str) -> str:
    """取最后一个 name: 匹配。
    理由：像 commands/insights.ts 里定义了多个 insight 子 section，每个都有 name:。
    真正的顶层 command 通常在 export default 附近（文件末尾）。
    对绝大多数只有一个 name: 的文件来说，first == last，无影响。
    """
    matches = NAME_RE.findall(text)
    return matches[-1] if matches else ""


def _stub_pattern_detected(text: str) -> bool:
    """检测 hard-cut 后遗留的 stub 模式：
    export default { isEnabled: () => false, isHidden: true, name: 'xxx' }
    通常是一行 .js 文件。见 P0 硬切历史。
    """
    return bool(
        re.search(r"isEnabled\s*:\s*\(\s*\)\s*=>\s*false", text)
        and re.search(r"isHidden\s*:\s*true", text)
    )


def scan_commands() -> list[CommandEntry]:
    entries: list[CommandEntry] = []
    # 遍历 commands/ 下所有 .ts/.tsx/.js 文件（含子目录 index.*，也含根级文件）
    candidates = (
        sorted(COMMANDS_DIR.rglob("*.ts"))
        + sorted(COMMANDS_DIR.rglob("*.tsx"))
        + sorted(COMMANDS_DIR.rglob("*.js"))
    )
    for path in candidates:
        # 跳过 types-only / prompt-only 子文件（保留 index 和顶层）
        if path.parent != COMMANDS_DIR and path.name not in {"index.ts", "index.tsx", "index.js"}:
            continue
        text = _text(path)
        if not text:
            continue

        name = _last_name_match(text)
        if not name:
            continue

        rel = str(path.relative_to(ROOT))
        is_stub = _stub_pattern_detected(text)

        if is_stub:
            kind = "command-stub"
        elif path.name in {"index.ts", "index.tsx", "index.js"}:
            kind = "command-dir"
        else:
            kind = "command-file"

        dir_name = path.parent.name if kind in ("command-dir", "command-stub") else ""

        entries.append(
            CommandEntry(
                name=name,
                kind=kind,
                source_path=rel,
                aliases=_extract_aliases(text),
                description=_extract_description(text),
                is_hidden=_is_hidden_state(text),
                availability=_availability(text),
                has_is_enabled=bool(IS_ENABLED_RE.search(text)),
                deferred_gate=_deferred_gate(text, name),
                feature_gates=_list_unique(FEATURE_RE, text),
                growthbook_flags=_list_unique(GROWTHBOOK_RE, text),
                is_stub=is_stub,
                dir_name=dir_name,
            )
        )

    # 去重策略：
    #   - 同一目录 .ts / .tsx / .js 可能并存 → 用 dir_name 作 key，选优先级最高的 kind
    #   - 根级 .ts/.tsx 没有 dir → 用 name 作 key
    #   - 注意：多个 stub 目录里 name 都是 'stub'，所以不能用 name 作跨目录的 key
    by_key: dict[str, CommandEntry] = {}
    priority = {"command-file": 0, "command-dir": 1, "command-stub": 2}
    for e in entries:
        key = f"dir:{e.dir_name}" if e.dir_name else f"name:{e.name}"
        existing = by_key.get(key)
        if existing is None or priority.get(e.kind, 99) < priority.get(existing.kind, 99):
            by_key[key] = e
    return sorted(by_key.values(), key=lambda e: (e.dir_name or e.name))


def scan_skills() -> list[CommandEntry]:
    entries: list[CommandEntry] = []
    for path in sorted(SKILLS_DIR.glob("*.ts")):
        if path.name in {"index.ts"}:
            continue
        text = _text(path)
        if not text:
            continue
        if not REGISTER_SKILL_RE.search(text):
            continue

        name_match = NAME_RE.search(text)
        if not name_match:
            continue
        name = name_match.group(1)

        ui_match = USER_INVOCABLE_RE.search(text)
        ui = ui_match.group(1) if ui_match else ""

        entries.append(
            CommandEntry(
                name=name,
                kind="skill",
                source_path=str(path.relative_to(ROOT)),
                description=_extract_description(text),
                has_is_enabled=bool(IS_ENABLED_RE.search(text)),
                feature_gates=_list_unique(FEATURE_RE, text),
                growthbook_flags=_list_unique(GROWTHBOOK_RE, text),
                user_invocable=ui,
            )
        )
    return entries


def _md_cell(s: str) -> str:
    # 转义 markdown 表格单元
    return (s or "").replace("|", "\\|").replace("\n", " ")[:160]


def _fmt_visibility_gates(e: CommandEntry) -> str:
    parts = []
    if e.is_hidden not in ("false", "?"):
        parts.append(f"isHidden={e.is_hidden}")
    if e.availability:
        parts.append(f"availability={e.availability}")
    if e.has_is_enabled:
        parts.append("isEnabled=fn")
    if e.deferred_gate:
        parts.append(f"deferred={e.deferred_gate}")
    if e.feature_gates:
        parts.append(f"feature={','.join(e.feature_gates)}")
    if e.growthbook_flags:
        parts.append(f"gb={','.join(e.growthbook_flags)}")
    return "; ".join(parts) if parts else "—"


def render_markdown(commands: list[CommandEntry], skills: list[CommandEntry]) -> str:
    live_commands = [c for c in commands if not c.is_stub]
    stub_commands = [c for c in commands if c.is_stub]
    user_invocable_skills = [s for s in skills if s.user_invocable == "true"]
    other_skills = [s for s in skills if s.user_invocable != "true"]

    live_total = len(live_commands) + len(user_invocable_skills)

    lines: list[str] = []
    lines.append("# Mossen Slash Command 全量 Inventory")
    lines.append("")
    lines.append(
        f"- **{live_total}** 条当前用户可见入口："
        f"{len(live_commands)} live commands + {len(user_invocable_skills)} userInvocable skills"
    )
    lines.append(f"- **{len(stub_commands)}** 条硬切后遗留的 stub 命令 (`isEnabled: () => false` + `isHidden: true`)")
    lines.append(
        f"- **{len(other_skills)}** 个 `userInvocable: false` 的 bundled skill（agent 内部使用，不暴露为 slash 命令）"
    )
    lines.append(f"- **合计 {len(commands) + len(skills)}** 条 registered entries")
    lines.append("")
    lines.append("自动生成。更新命令：")
    lines.append("")
    lines.append("```bash")
    lines.append("python3 scripts/command_inventory.py > docs/command_inventory.md")
    lines.append("```")
    lines.append("")
    lines.append(
        "架构参考：MOSSEN.md §3.4（两条注册路径）、§3.5（七种可见性门）、"
        "`utils/deferredSlashCommands.ts`、`utils/customBackend.ts`。"
    )
    lines.append("")
    lines.append("待人工补填（P1-5 slice B，用户本机完成）的四列：")
    lines.append("")
    lines.append("- **依赖 CB**：是否依赖 custom backend 才能工作 (Y/N)")
    lines.append("- **依赖本地**：是否依赖本地文件 / git / worktree (Y/N)")
    lines.append("- **依赖 hosted**：是否依赖 hosted 服务 (Y/N)")
    lines.append("- **结论**：保留 / hidden / deferred / 其他（含 bug 单链接）")
    lines.append("")

    # Section 1: live commands
    lines.append("## 1. Live `commands/` — 用户可见或条件可见")
    lines.append("")
    lines.append(f"共 {len(live_commands)} 条。")
    lines.append("")
    lines.append("| # | 命令 | 别名 | 路径 | 可见性门 | 描述 | 依赖CB | 依赖本地 | 依赖hosted | 结论 |")
    lines.append("|---|------|------|------|---------|------|:-----:|:-------:|:---------:|------|")
    for i, e in enumerate(live_commands, 1):
        lines.append(
            "| {i} | `/{name}` | {aliases} | `{path}` | {gates} | {desc} |  |  |  |  |".format(
                i=i,
                name=e.name,
                aliases=", ".join(f"`/{a}`" for a in e.aliases) if e.aliases else "—",
                path=_md_cell(e.source_path),
                gates=_md_cell(_fmt_visibility_gates(e)),
                desc=_md_cell(e.description) or "—",
            )
        )
    lines.append("")

    # Section 2: userInvocable skills
    lines.append("## 2. `skills/bundled/*.ts` — userInvocable: true")
    lines.append("")
    lines.append(f"共 {len(user_invocable_skills)} 条。这些走 skills 注册路径，不在 commands/ 目录；P0 复查时误判过（MOSSEN.md §3.4）。")
    lines.append("")
    lines.append("| # | 命令 | 路径 | 门控 | 描述 | 依赖CB | 依赖本地 | 依赖hosted | 结论 |")
    lines.append("|---|------|------|------|------|:-----:|:-------:|:---------:|------|")
    for i, e in enumerate(user_invocable_skills, 1):
        lines.append(
            "| {i} | `/{name}` | `{path}` | {gates} | {desc} |  |  |  |  |".format(
                i=i,
                name=e.name,
                path=_md_cell(e.source_path),
                gates=_md_cell(_fmt_visibility_gates(e)),
                desc=_md_cell(e.description) or "—",
            )
        )
    lines.append("")

    # Section 3: stub commands
    lines.append("## 3. Stub `commands/*/index.js` — 硬切后遗留")
    lines.append("")
    lines.append(
        f"共 {len(stub_commands)} 条。模式 `export default {{ isEnabled: () => false, isHidden: true, name: '...' }}`。"
        "这些是官方原来有、Mossen 硬切时为了保留 import 链不炸才留的 placeholder；用户永远看不到，可作为"
        "hidden 命令清单的审计参照（未来要么补 Mossen 版，要么彻底删）。"
    )
    lines.append("")
    lines.append("| # | 目录 | 内部 name | 路径 |")
    lines.append("|---|------|-----------|------|")
    for i, e in enumerate(stub_commands, 1):
        lines.append(
            "| {i} | `commands/{dir}/` | `{name}` | `{path}` |".format(
                i=i,
                dir=e.dir_name or "-",
                name=e.name,
                path=_md_cell(e.source_path),
            )
        )
    lines.append("")

    # Section 4: non-userInvocable bundled skills (informational)
    lines.append("## 4. `skills/bundled/*.ts` — userInvocable: false（非用户入口）")
    lines.append("")
    lines.append(f"共 {len(other_skills)} 条。这些 skill 只被 agent 内部或其他代码调用，不暴露为 slash 命令。")
    lines.append("")
    lines.append("| # | 名称 | 路径 | 描述 |")
    lines.append("|---|------|------|------|")
    for i, e in enumerate(other_skills, 1):
        lines.append(
            "| {i} | `{name}` | `{path}` | {desc} |".format(
                i=i,
                name=e.name,
                path=_md_cell(e.source_path),
                desc=_md_cell(e.description) or "—",
            )
        )
    lines.append("")

    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", help="输出 JSON 供下游工具消费")
    parser.add_argument("--count", action="store_true", help="仅打印数量统计")
    args = parser.parse_args()

    commands = scan_commands()
    skills = scan_skills()

    if args.count:
        user_invocable = sum(1 for s in skills if s.user_invocable == "true")
        live_commands = sum(1 for c in commands if not c.is_stub)
        stub_commands = sum(1 for c in commands if c.is_stub)
        print(f"commands (live): {live_commands}")
        print(f"commands (stub): {stub_commands}")
        print(f"skills (userInvocable=true): {user_invocable}")
        print(f"skills (userInvocable=false): {len(skills) - user_invocable}")
        print(f"user-visible slash entries total: {live_commands + user_invocable}")
        print(f"all registered entries: {len(commands) + len(skills)}")
        return 0

    if args.json:
        print(
            json.dumps(
                {
                    "commands": [asdict(c) for c in commands],
                    "skills": [asdict(s) for s in skills],
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 0

    print(render_markdown(commands, skills))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
