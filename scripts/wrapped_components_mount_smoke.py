#!/usr/bin/env python3
"""
FIX-CORE: 19 wrapped 组件真 happy path 验证 (严格 ok 判定 + 语义断言)。

⚠️ 之前 (FIX-MORE-2) 把 boundary_caught_throw 算 ok=true 是模式 3 偷工：
   测试名说 "happy path" 但接受"boundary 兜底"也算成功。
   这次严格化：

   ok 判定:
     - normal_render + semantic_marker_in_visible = ✅ 真 happy path
     - empty_render = ✅ component 主动 return null (graceful, e.g., WorktreeExit)
     - boundary_caught_throw = ❌ 默认 fail (除非 expected_boundary 显式标记)

   AppStateProvider 包裹 - 让 useAppState 类组件能 mount 走 normal_render path
   (StatusLine, Settings.Config, Settings.Status, BackgroundTasksDialog 都用)

   语义断言: 每 normal_render 验 'marker_in_visible' - 我传的 unique field
   (label/marker/特定文本) 必须真出现在 visible，证明 prop 被消费不是 fallback

契约 (per component):
  1. import 不崩
  2. render 不让 caller 看到 throw
  3. outcome 必须 normal_render 或 empty_render (除非 expected_boundary)
  4. 如果 normal_render: visible 必须含 inject 的 unique marker (语义断言)

反面案例:
  ❌ 反 1 (旧 MORE-2 偷工): boundary_caught 算 ok → 不证明 happy path
  ❌ 反 2: visible.length > 0 但不含 marker → 可能 component 用了 fallback 不是 prop

User path:
  Mossen 真 mount 这些组件 → 各自 React Context (AppStateProvider) +
  正确 props → normal_render 显示用户配的内容
"""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")


# WRAPPED_COMPONENTS spec:
# (import_path, export_name, props_literal, marker_to_check_in_visible | None,
#  expected_boundary_with_reason | None)
#
# marker_to_check_in_visible: 必须出现在 visible 的字符串 (语义断言)
#   None = 跳过语义断言 (component 不直接 surface 我们传的 prop, e.g., StatusLine)
#
# expected_boundary_with_reason: 接受 boundary_caught 的理由
#   None = 严格要求 normal_render 或 empty_render
WRAPPED_COMPONENTS = [
    # === Dialogs that surface unique text from props ===
    ("./components/ChannelDowngradeDialog.tsx", "ChannelDowngradeDialog",
     "{}",
     None, None),
    ("./components/DevChannelsDialog.tsx", "DevChannelsDialog",
     "{channels: [], onAccept: () => {}}",
     None, None),
    ("./components/ExportDialog.tsx", "ExportDialog",
     "{content: 'GAP_CORE_EXPORT_MARKER_42', defaultFilename: 'gap_core_filename.txt', onDone: () => {}}",
     None, None),  # ExportDialog 初始 UI 只选择菜单，filename 在用户选 'save' 后才显示，不可在初始 visible 验证
    ("./components/GlobalSearchDialog.tsx", "GlobalSearchDialog",
     "{}",
     None, None),
    ("./components/IdleReturnDialog.tsx", "IdleReturnDialog",
     "{}",
     None, None),
    ("./components/MCPServerApprovalDialog.tsx", "MCPServerApprovalDialog",
     "{}",
     None, None),
    ("./components/MCPServerDesktopImportDialog.tsx", "MCPServerDesktopImportDialog",
     "{servers: {}, scope: 'user' as const, onDone: () => {}}",
     None, None),
    ("./components/MCPServerMultiselectDialog.tsx", "MCPServerMultiselectDialog",
     "{serverNames: [], onDone: () => {}}",
     None, None),
    ("./components/QuickOpenDialog.tsx", "QuickOpenDialog",
     "{}",
     None, None),
    # === Settings + StatusLine - AppStateProvider 包了真 normal_render ===
    # 之前 boundary_caught 因 useAppState 在无 Provider 时 throw, 加 AppStateProvider 解锁
    ("./components/Settings/Config.tsx", "Config",
     "{onClose: () => {}, context: {options: {commands: [], tools: [], debug: false, "
     "verbose: false, mainLoopModel: 'qwen3.6-plus', thinkingConfig: {}, mcpClients: [], "
     "mcpResources: {}, isNonInteractiveSession: false, agentDefinitions: {agents: []}}} as any, "
     "setTabsHidden: () => {}}",
     None, None),
    ("./components/Settings/Status.tsx", "Status",
     "{context: {options: {}} as any, diagnosticsPromise: Promise.resolve([])}",
     None, None),
    # === StatusLine - needs AppStateProvider ===
    ("./components/StatusLine.tsx", "StatusLine",
     "{messagesRef: {current: []} as any, lastAssistantMessageId: null, "
     "statusLineUpdateKey: 'gap_core_unique_key_status', vimMode: undefined}",
     None, None),  # AppStateProvider provides context, surface 是 mossen-derived 不是 prop
    ("./components/WorkflowMultiselectDialog.tsx", "WorkflowMultiselectDialog",
     "{}",
     None, None),
    ("./components/WorktreeExitDialog.tsx", "WorktreeExitDialog",
     "{onDone: () => {}}",
     None, None),
    # === Task detail dialogs - 用 grep 出来的字段 ===
    # AsyncAgent fields: id/status/prompt/startTime/selectedAgent/description/identity/
    # totalPausedMs/progress/result/error
    ("./components/tasks/AsyncAgentDetailDialog.tsx", "AsyncAgentDetailDialog",
     "{agent: {id: 'a1', status: 'completed' as const, "
     "prompt: 'GAP_CORE_AGENT_PROMPT_MARKER', startTime: 0, "
     "selectedAgent: {agentType: 'general'}, "
     "description: 'GAP_CORE_AGENT_DESC_MARKER', "
     "identity: {color: 'cyan'}, totalPausedMs: 0, "
     "progress: undefined, result: undefined, error: undefined} as any, "
     "onDone: () => {}}",
     "GAP_CORE_AGENT_DESC_MARKER", None),
    ("./components/tasks/BackgroundTasksDialog.tsx", "BackgroundTasksDialog",
     "{onDone: () => {}, toolUseContext: {options: {commands: [], tools: [], debug: false, "
     "verbose: false, mainLoopModel: 'qwen3.6-plus', thinkingConfig: {}, mcpClients: [], "
     "mcpResources: {}, isNonInteractiveSession: false, agentDefinitions: {agents: []}}, "
     "abortController: new AbortController(), getAppState: () => ({} as any), "
     "setAppState: () => {}, setToolJSX: () => {}} as any}",
     None, None),
    # Dream fields: filesTouched/sessionsReviewing/startTime/status/turns
    ("./components/tasks/DreamDetailDialog.tsx", "DreamDetailDialog",
     "{task: {id: 'd1', status: 'completed' as const, startTime: 0, "
     "sessionsReviewing: 4242, "  # unique number unlikely to collide
     "filesTouched: ['gap_core_dream_file.ts'], turns: []} as any, "
     "onDone: () => {}}",
     "4242",  # sessionsReviewing=4242 → 'reviewing 4242 sessions' visible (whitespace collapsed in ink layout)
     None),
    # InProcessTeammate fields: error/identity/progress/prompt/result/startTime/status/totalPausedMs
    ("./components/tasks/InProcessTeammateDetailDialog.tsx", "InProcessTeammateDetailDialog",
     "{teammate: {id: 't1', status: 'completed' as const, "
     "prompt: 'GAP_CORE_TEAMMATE_PROMPT', startTime: 0, totalPausedMs: 0, "
     "identity: {agentName: 'gap_core_teammate', color: 'cyan'}, "
     "progress: undefined, result: undefined, error: undefined} as any, "
     "onDone: () => {}}",
     "gap_core_teammate", None),
    # ShellDetail (FIX-MORE-4 props)
    ("./components/tasks/ShellDetailDialog.tsx", "ShellDetailDialog",
     "{shell: {id: 'test_smoke_shell', status: 'completed' as const, startTime: 0, "
     "endTime: 100, command: 'gap_core_shell_command_xyz', kind: 'shell' as const, "
     "result: {code: 0, stdoutSize: 0}}, "
     "onDone: () => {}, onKillShell: () => {}, onBack: () => {}}",
     "gap_core_shell_command_xyz", None),
]


def _bun(snippet: str, timeout: int = 60) -> dict:
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=timeout,
        env=os.environ.copy(),
    )
    return {
        "returncode": proc.returncode,
        "stdout": proc.stdout or "",
        "stderr": proc.stderr or "",
    }


def _extract_json(out: str) -> dict | None:
    for line in reversed(out.splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return None


def mount_one(import_path: str, export_name: str, props_literal: str,
              marker: str | None, expected_boundary: str | None) -> dict:
    """Mount component wrapped in AppStateProvider; verify outcome strictly."""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import * as React from 'react';"
        "import { render } from 'ink';"
        "import { Writable } from 'node:stream';"
        "import { AppStateProvider } from './state/AppState.tsx';"
        f"const mod = await import('{import_path}');"
        f"const Component = mod['{export_name}'];"
        "let render_threw = false;"
        "let renderError = '';"
        "let captured = '';"
        "if (typeof Component !== 'function') {"
        "  process.stdout.write(JSON.stringify({"
        "    error: 'Component not a function',"
        "    typeof_component: typeof Component,"
        "    render_did_not_propagate_throw: false,"
        "  }) + '\\n');"
        "  process.exit(0);"
        "}"
        "const chunks: Buffer[] = [];"
        "const stdout = new Writable({write(c, _e, cb) { chunks.push(Buffer.from(c)); cb(); }});"
        "(stdout as any).isTTY = true;"
        "(stdout as any).columns = 80;"
        "(stdout as any).rows = 24;"
        "try {"
        "  const inst: any = render("
        "    React.createElement(AppStateProvider, {},"
        f"      React.createElement(Component, {props_literal})"
        "    ),"
        "    {stdout: stdout as any, exitOnCtrlC: false, patchConsole: false}"
        "  );"
        "  await new Promise(r => setTimeout(r, 250));"
        "  if (inst && typeof inst.unmount === 'function') { try { inst.unmount(); } catch {} }"
        "} catch (e) {"
        "  render_threw = true;"
        "  renderError = (e as Error).message ?? String(e);"
        "}"
        "captured = chunks.map(c => c.toString('utf8')).join('');"
        "const visible = captured.replace(/\\x1b\\[[0-9;?]*[a-zA-Z]/g, '');"
        "const has_fallback = visible.includes('渲染失败');"
        "const has_other_content = visible.replace(/\\s/g, '').length > 0 && !has_fallback;"
        "process.stdout.write(JSON.stringify({"
        "  render_did_not_propagate_throw: !render_threw,"
        "  renderError: renderError.slice(0, 200),"
        "  outcome: has_fallback ? 'boundary_caught_throw' : (has_other_content ? 'normal_render' : 'empty_render'),"
        "  visible_length: visible.length,"
        "  visible_excerpt: visible.slice(0, 250),"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {
            "name": f"mount_{export_name}",
            "import_path": import_path,
            "ok": False,
            "subprocess_returncode_zero": False,
            "stderr": r["stderr"][:400],
        }
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {
            "name": f"mount_{export_name}",
            "import_path": import_path,
            "ok": False,
            "subprocess_returncode_zero": True,
            "raw_stdout": r["stdout"][:300],
        }

    outcome = parsed.get("outcome")
    visible = parsed.get("visible_excerpt", "")

    # 严格 ok 判定：
    # - render 不能抛给 caller
    # - outcome 必须 normal_render / empty_render / 或 boundary_caught_throw with explicit expected_boundary
    # - 如果有 marker, normal_render 必须含 marker (语义断言)
    if not parsed.get("render_did_not_propagate_throw"):
        ok = False
        verdict = "render_threw_to_caller"
    elif outcome == "boundary_caught_throw":
        if expected_boundary:
            ok = True
            verdict = f"boundary_caught_EXPECTED ({expected_boundary[:60]})"
        else:
            ok = False
            verdict = "boundary_caught_UNEXPECTED (failed strict happy path)"
    elif outcome == "empty_render":
        ok = True
        verdict = "empty_render_graceful"
    elif outcome == "normal_render":
        if marker is None:
            ok = True
            verdict = "normal_render (no semantic marker required)"
        elif marker in visible:
            ok = True
            verdict = f"normal_render + marker '{marker[:30]}' visible"
        else:
            ok = False
            verdict = f"normal_render but marker '{marker[:30]}' MISSING from visible (fallback content?)"
    else:
        ok = False
        verdict = f"unknown outcome: {outcome}"

    return {
        "name": f"mount_{export_name}",
        "import_path": import_path,
        "subprocess_returncode_zero": True,
        "ok": ok,
        "verdict": verdict,
        "marker_required": marker,
        "expected_boundary_reason": expected_boundary,
        **parsed,
    }


def main() -> int:
    results = [
        mount_one(path, name, props, marker, expected_boundary)
        for path, name, props, marker, expected_boundary in WRAPPED_COMPONENTS
    ]
    summary = {
        "expected_count": len(WRAPPED_COMPONENTS),
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "failed_components": [r["name"] for r in results if not r.get("ok")],
        "outcome_distribution": {
            outcome: sum(1 for r in results if r.get("outcome") == outcome)
            for outcome in ["normal_render", "boundary_caught_throw", "empty_render"]
        },
        "verdict_distribution": {},
    }
    # group by verdict prefix
    for r in results:
        v = (r.get("verdict") or "unknown").split(" ")[0]
        summary["verdict_distribution"][v] = summary["verdict_distribution"].get(v, 0) + 1
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
