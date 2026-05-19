#!/usr/bin/env python3
"""
GAP 5: 19 个 withErrorBoundary 包过的组件 module-load + export 验证。

⚠️ 之前批量 sed 改 14 文件 + 5 task dialogs，靠 typecheck:diff 通过判定"成功"。
但 typecheck 不验 runtime import；如果 sed 改坏 export 名 / 模块解析失败，
typecheck 可能漏掉但 runtime 起 mossen 会崩。本 smoke 真 import 每个。

契约（4 条 falsifiable per component）：
  1. import { <Name> } from <path> 不 throw
  2. <Name> 是 function（React FC）
  3. <Name>.displayName 含 'withErrorBoundary' 前缀（HOC 标记）
  4. 总数 = 19 个文件级 wrap

反面案例：
  ❌ 反 1: 只 grep 文件含 withErrorBoundary 字符串 → 不证 module-load 成功
  ❌ 反 2: 只 import 不验 displayName → 可能 export 是别的名字

User path:
  Mossen 主入口 → import { Dialog } from './components/...' →
  实际拿到 withErrorBoundary 包过的 wrapper FC → 渲染时 boundary 生效

Mutation point:
  改一个组件的 `export const X = withErrorBoundary(XImpl, 'X')` 为
  `export const X = XImpl as any` → displayName 不再含 withErrorBoundary
  → 测试该组件 fail
"""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")

# 19 文件级 wrapped 组件（手动列；与 grep 结果一致）
# 格式: (import_path_relative_to_root, export_name)
WRAPPED_COMPONENTS = [
    ("./components/ChannelDowngradeDialog.tsx", "ChannelDowngradeDialog"),
    ("./components/DevChannelsDialog.tsx", "DevChannelsDialog"),
    ("./components/ExportDialog.tsx", "ExportDialog"),
    ("./components/GlobalSearchDialog.tsx", "GlobalSearchDialog"),
    ("./components/IdleReturnDialog.tsx", "IdleReturnDialog"),
    ("./components/MCPServerApprovalDialog.tsx", "MCPServerApprovalDialog"),
    ("./components/MCPServerDesktopImportDialog.tsx", "MCPServerDesktopImportDialog"),
    ("./components/MCPServerMultiselectDialog.tsx", "MCPServerMultiselectDialog"),
    ("./components/QuickOpenDialog.tsx", "QuickOpenDialog"),
    ("./components/Settings/Config.tsx", "Config"),
    ("./components/Settings/Status.tsx", "Status"),
    ("./components/StatusLine.tsx", "StatusLine"),
    ("./components/WorkflowMultiselectDialog.tsx", "WorkflowMultiselectDialog"),
    ("./components/WorktreeExitDialog.tsx", "WorktreeExitDialog"),
    ("./components/tasks/AsyncAgentDetailDialog.tsx", "AsyncAgentDetailDialog"),
    ("./components/tasks/BackgroundTasksDialog.tsx", "BackgroundTasksDialog"),
    ("./components/tasks/DreamDetailDialog.tsx", "DreamDetailDialog"),
    ("./components/tasks/InProcessTeammateDetailDialog.tsx", "InProcessTeammateDetailDialog"),
    ("./components/tasks/ShellDetailDialog.tsx", "ShellDetailDialog"),
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


def check_one(import_path: str, export_name: str) -> dict:
    """Import a single wrapped component and verify export shape."""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "let importError: string | null = null;"
        "let mod: any = null;"
        "try {"
        f"  mod = await import('{import_path}');"
        "} catch (e) {"
        "  importError = (e as Error).message ?? String(e);"
        "}"
        f"const exportValue = mod ? mod['{export_name}'] : undefined;"
        "process.stdout.write(JSON.stringify({"
        "  import_succeeded: importError === null,"
        "  importError,"
        "  export_present: exportValue !== undefined,"
        "  export_is_function: typeof exportValue === 'function',"
        "  has_displayName: exportValue && typeof exportValue.displayName === 'string',"
        "  displayName: exportValue?.displayName ?? null,"
        "  isWrapped: exportValue?.displayName?.startsWith('withErrorBoundary') === true,"
        "}) + '\\n');"
    )
    r = _bun(snippet)
    if r["returncode"] != 0:
        return {
            "name": f"import_{export_name}",
            "ok": False,
            "import_path": import_path,
            "stderr": r["stderr"][:300],
        }
    parsed = _extract_json(r["stdout"])
    if parsed is None:
        return {
            "name": f"import_{export_name}",
            "ok": False,
            "import_path": import_path,
            "raw_stdout": r["stdout"][:300],
        }
    return {
        "name": f"import_{export_name}",
        "import_path": import_path,
        "ok": (
            parsed.get("import_succeeded") is True
            and parsed.get("export_is_function") is True
            and parsed.get("isWrapped") is True
        ),
        **parsed,
    }


def main() -> int:
    expected_count = 19
    if len(WRAPPED_COMPONENTS) != expected_count:
        print(json.dumps({
            "error": f"WRAPPED_COMPONENTS list length {len(WRAPPED_COMPONENTS)} != expected {expected_count}",
        }, indent=2))
        return 1

    results = []
    for import_path, export_name in WRAPPED_COMPONENTS:
        results.append(check_one(import_path, export_name))

    summary = {
        "expected_count": expected_count,
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "failed_components": [r["name"] for r in results if not r.get("ok")],
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
