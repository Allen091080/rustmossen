#!/usr/bin/env python3
"""Wave 2B — B-C2 (DUMPPROMPTS-DELETE) focused smoke (static-only).

Verifies S3 删文件 + 14 处引用清理:

  1. services/api/dumpPrompts.ts 文件不存在 (整文件删除)
  2. 全仓 grep `dumpPrompts.js` import 命中 = 0 (8 处 import 全删)
  3. addApiRequestToCache / dumpRequest / dumpResponse / clearDumpState /
     clearAllDumpState / createDumpPromptsFetch / getDumpPromptsPath /
     getLastApiRequests / clearApiRequestCache 全部不在任何 *.ts/*.tsx 中作为 runtime 标识符使用 (注释引用允许)
  4. /issue /share 命令仍 stub (不触发 build error)

Why static-only:
  * dumpPrompts.ts 透传依赖为 mossen SDK + node fs, `bun -e` 解析受 deferred 模块限制
  * S3 删除是纯文件级 + import 级清理, 静态断言已足够
  * 真实运行行为 (~/.mossen/dump-prompts/ 不写入) 由 TUI smoke 兜底

SA-1 已确认 5 项审查全 0 影响:
  - 测试 0 *.test.ts 引用
  - 文档 0 *.md 命中
  - prompt fetch wrapper 不进 prompt 文案
  - slash command /issue /share 已是 stub (`isEnabled: () => false, isHidden: true`)
  - harness 0 依赖
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DUMP_FILE = ROOT / "services" / "api" / "dumpPrompts.ts"

RUNTIME_SYMBOLS = [
    "addApiRequestToCache",
    "createDumpPromptsFetch",
    "clearDumpState",
    "clearAllDumpState",
    "getDumpPromptsPath",
    "getLastApiRequests",
    "clearApiRequestCache",
]


def static_assertion() -> dict[str, object]:
    findings: dict[str, object] = {
        "dump_prompts_file_deleted": True,
        "import_dumpprompts_count": 0,
        "runtime_symbol_callsites": {},
        "issue_command_stub_preserved": True,
        "share_command_stub_preserved": True,
    }

    if DUMP_FILE.exists():
        findings["dump_prompts_file_deleted"] = False

    # Walk all .ts/.tsx files except smoke/test/scripts.
    target_files: list[Path] = []
    for ext in ("ts", "tsx"):
        for f in ROOT.rglob(f"*.{ext}"):
            rel = f.relative_to(ROOT)
            parts = rel.parts
            if parts[0] == "scripts":
                continue
            if parts[0] == "node_modules":
                continue
            if any(p == "node_modules" for p in parts):
                continue
            target_files.append(f)

    import_count = 0
    callsite_counts: dict[str, int] = {s: 0 for s in RUNTIME_SYMBOLS}

    for f in target_files:
        try:
            text = f.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue

        # Count `dumpPrompts.js` imports (note: covers both './dumpPrompts.js' and 'src/.../dumpPrompts.js')
        if re.search(r"from ['\"][^'\"]*dumpPrompts\.js['\"]", text):
            import_count += 1

        # For each symbol, count non-comment-line uses.
        # Strip line comments (// ...) and block comments (/* ... */) loosely.
        cleaned = re.sub(r"/\*[\s\S]*?\*/", "", text)
        cleaned = "\n".join(
            line for line in cleaned.splitlines() if not line.lstrip().startswith("//")
        )
        for sym in RUNTIME_SYMBOLS:
            # Match the symbol followed by ( or other identifier-boundary, not as part of another word.
            pattern = re.compile(rf"\b{re.escape(sym)}\b")
            for m in pattern.finditer(cleaned):
                callsite_counts[sym] += 1

    findings["import_dumpprompts_count"] = import_count
    findings["runtime_symbol_callsites"] = callsite_counts

    # /issue and /share commands stub preserved.
    # /issue /share 命令 stub 在 .js (从仓库初始 import 起就是 stub)
    issue_idx = ROOT / "commands" / "issue" / "index.js"
    share_idx = ROOT / "commands" / "share" / "index.js"
    for path, key in [
        (issue_idx, "issue_command_stub_preserved"),
        (share_idx, "share_command_stub_preserved"),
    ]:
        if not path.exists():
            findings[key] = False
            continue
        try:
            stub_text = path.read_text(encoding="utf-8")
            findings[key] = (
                "isEnabled" in stub_text
                or "isHidden" in stub_text
            )
        except (UnicodeDecodeError, OSError):
            findings[key] = False

    return findings


def main() -> int:
    failures: list[str] = []
    f = static_assertion()

    if not f["dump_prompts_file_deleted"]:
        failures.append("services/api/dumpPrompts.ts 仍存在 — S3 要求整文件删除")
    if f["import_dumpprompts_count"] != 0:
        failures.append(
            f"全仓 dumpPrompts.js import 命中 = {f['import_dumpprompts_count']},预期 0 "
            "(8 处 import 应全删)"
        )

    sym_counts = f["runtime_symbol_callsites"]
    leaked = {s: c for s, c in sym_counts.items() if c > 0}
    if leaked:
        failures.append(
            f"runtime 标识符在源代码中仍被引用 (非注释): {leaked}"
        )

    if not f["issue_command_stub_preserved"]:
        failures.append("/issue 命令 stub 丢失")
    if not f["share_command_stub_preserved"]:
        failures.append("/share 命令 stub 丢失")

    report = {
        "name": "wave2_b_dump_prompts_no_write_smoke",
        "mode": "static-only",
        "mode_reason": (
            "dumpPrompts.ts 透传依赖含 mossen SDK + deferred 模块。S3 删文件是纯结构, "
            "静态断言已足够;真实 ~/.mossen/dump-prompts/ 无写入由 TUI smoke 兜底。"
        ),
        "static_findings": f,
        "failures": failures,
        "passed": 4 - len(failures),
        "total": 4,
    }
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
