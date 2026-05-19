#!/usr/bin/env python3
"""Wave 2A — A3 (CCR isolation:'remote' hard remove) focused smoke (static-only).

Verifies S3 hard remove:
  * tools/AgentTool/prompt.ts USER_TYPE === 'mossen' 'remote' 文档段已删
  * tools/AgentTool/loadAgentsDir.ts zod schema 简化为 z.enum(['worktree']).optional()
  * tools/AgentTool/loadAgentsDir.ts VALID_ISOLATION_MODES = ['worktree'] (无 'remote')
  * tools/AgentTool/AgentTool.tsx RemoteLaunchedOutput type 已删 + UI.tsx import 已修
  * tools/AgentTool/AgentTool.tsx 'remote_launched' 字面量已 0 命中 (UI.tsx 仍残留 — Wave 3)

Why static-only:
  * v3 §2.4 case 设计为静态结构断言:每个 case 直接检查 *.ts 内特定字符串/语法
    的存在/缺失;真实运行行为由 TUI 集成 smoke 兜底。

case 草案 (v3 §2.4 + Allen 方案 1 修正):
  A: prompt.ts 不含 'USER_TYPE === \\'mossen\\'' (远程文档段 0 命中)
  B: loadAgentsDir.ts zod schema 是 z.enum(['worktree']).optional() (无 'remote')
  C: VALID_ISOLATION_MODES 集合 = ['worktree']
  D: AgentTool.tsx 不含 RemoteLaunchedOutput type 定义,UI.tsx import 中也不含
  E: AgentTool.tsx 不含 'remote_launched' 字面量 (UI.tsx 仍有 2 处 — Allen 划的边界)
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROMPT_TS = ROOT / "tools" / "AgentTool" / "prompt.ts"
LOAD_TS = ROOT / "tools" / "AgentTool" / "loadAgentsDir.ts"
AGENT_TSX = ROOT / "tools" / "AgentTool" / "AgentTool.tsx"
UI_TSX = ROOT / "tools" / "AgentTool" / "UI.tsx"


def static_assertion() -> dict[str, object]:
    prompt = PROMPT_TS.read_text(encoding="utf-8")
    load = LOAD_TS.read_text(encoding="utf-8")
    agent = AGENT_TSX.read_text(encoding="utf-8")
    ui = UI_TSX.read_text(encoding="utf-8")

    findings: dict[str, object] = {}

    # Case A: prompt.ts 不含 USER_TYPE === 'mossen' (远程文档段已删)
    case_a_hits = re.findall(r"USER_TYPE\s*===\s*'mossen'", prompt)
    findings["case_A_prompt_no_mossen_gate"] = len(case_a_hits) == 0
    findings["_case_A_hits"] = len(case_a_hits)

    # Case B: loadAgentsDir.ts zod schema = z.enum(['worktree']).optional()
    # 同时不能含 z.enum(['worktree', 'remote'])
    case_b_correct = (
        "isolation: z.enum(['worktree']).optional()" in load
        or 'isolation: z.enum(["worktree"]).optional()' in load
    )
    case_b_no_remote = "z.enum(['worktree', 'remote'])" not in load
    findings["case_B_load_schema_simplified"] = case_b_correct and case_b_no_remote

    # Case C: VALID_ISOLATION_MODES = ['worktree']
    case_c_correct = (
        "VALID_ISOLATION_MODES: readonly IsolationMode[] = ['worktree']" in load
        or "VALID_ISOLATION_MODES: readonly IsolationMode[] = ['worktree']\n" in load
    )
    case_c_no_remote_array = "['worktree', 'remote']" not in load
    findings["case_C_valid_modes_only_worktree"] = case_c_correct and case_c_no_remote_array

    # Case D: AgentTool.tsx 不含 export type RemoteLaunchedOutput,
    # UI.tsx import 不含 RemoteLaunchedOutput
    case_d_no_def = "export type RemoteLaunchedOutput" not in agent
    case_d_ui_import_clean = re.search(
        r"import type \{[^}]*RemoteLaunchedOutput[^}]*\} from '\./AgentTool\.js'", ui
    ) is None
    findings["case_D_RemoteLaunchedOutput_type_removed"] = case_d_no_def and case_d_ui_import_clean

    # Case E: AgentTool.tsx 不含 'remote_launched' 字面量
    # UI.tsx 仍有 2 处 ('remote_launched' string + outputStatus === 'remote_launched') — Allen 划的边界
    agent_remote_launched = agent.count("'remote_launched'") + agent.count('"remote_launched"')
    ui_remote_launched = ui.count("'remote_launched'") + ui.count('"remote_launched"')
    findings["case_E_AgentTool_no_remote_launched"] = agent_remote_launched == 0
    findings["_case_E_ui_remote_launched_hits"] = ui_remote_launched
    findings["_case_E_ui_boundary_note"] = "UI.tsx 'remote_launched' 字面量留 Wave 3 物理删 .tsx 时清 (Allen 方案 1 边界)"

    return findings


def main() -> int:
    findings = static_assertion()
    cases = ["case_A_prompt_no_mossen_gate", "case_B_load_schema_simplified",
             "case_C_valid_modes_only_worktree", "case_D_RemoteLaunchedOutput_type_removed",
             "case_E_AgentTool_no_remote_launched"]
    passed = sum(1 for c in cases if findings.get(c) is True)
    total = len(cases)
    findings["_summary"] = f"{passed}/{total} PASS"

    print(json.dumps(findings, indent=2, ensure_ascii=False))

    if passed == total:
        print(f"\n[PASS] Wave 2A-A3 smoke: {passed}/{total}")
        return 0
    print(f"\n[FAIL] Wave 2A-A3 smoke: {passed}/{total}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
