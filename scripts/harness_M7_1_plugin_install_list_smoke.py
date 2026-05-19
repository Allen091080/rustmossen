#!/usr/bin/env python3
"""
M7.1 — plugin install + list e2e (--plugin-dir 路径).

按 harness全链路测试.md §3.7 M7.1 契约:
  前置: fixture root 下创建 mock plugin (plugin.json + commands/<cmd>.md)
  步骤: setInlinePlugins([dir]) → loadAllPluginsCacheOnly() 真把 plugin 装上
  观察点:
    1. enabled 列表里出现 mock plugin name (mock_plugin_M7_1)
    2. plugin.source 含 '@inline' (确认走 --plugin-dir 路径, 不是 marketplace)
    3. plugin.commandsPath 指向 commands/ (真发现 commands 目录)
    4. getPluginCommands() 返回的 cmd 列表含 'mock_plugin_M7_1:mock_cmd_M7_1'
  反测信号: 注释 src/main.tsx:969 setInlinePlugins(pluginDir) 调用 →
            getInlinePlugins() 返回 [] → loadSessionOnlyPlugins 不跑 →
            mock plugin 不在 enabled, command 列表少 → fail

实现策略 (方式 C):
  bun -e snippet 模拟 mossen 启动时 setInlinePlugins → 调真 plugin loader
  真实导出名 (已 grep 验证):
    setInlinePlugins  ← src/bootstrap/state.ts:1255
    getInlinePlugins  ← src/bootstrap/state.ts:1259
    loadAllPluginsCacheOnly ← src/utils/plugins/pluginLoader.ts:3137
    getPluginCommands ← src/utils/plugins/loadPluginCommands.ts:414
  Manifest 写到 .mossen-plugin/plugin.json (createPluginFromPath:1359 真路径).
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_BUN = str(ROOT / "run-bun-featured.sh")

PLUGIN_NAME = "mock_plugin_M7_1"
COMMAND_BASENAME = "mock_cmd_M7_1"
EXPECTED_FULL_CMD_NAME = f"{PLUGIN_NAME}:{COMMAND_BASENAME}"
COMMAND_BODY_MARKER = "PLUGIN_M7_2_RAN"  # 同一 fixture body, M7.2 也要用


def _build_mock_plugin(plugin_dir: Path) -> None:
    """按 createPluginFromPath 的真路径要求建 mock plugin 文件结构。"""
    manifest_dir = plugin_dir / ".mossen-plugin"
    manifest_dir.mkdir(parents=True, exist_ok=True)
    (manifest_dir / "plugin.json").write_text(
        json.dumps(
            {
                "name": PLUGIN_NAME,
                "version": "0.0.1",
                "description": "M7.1 fixture test plugin",
            },
            indent=2,
        ),
        encoding="utf-8",
    )

    commands_dir = plugin_dir / "commands"
    commands_dir.mkdir(parents=True, exist_ok=True)
    (commands_dir / f"{COMMAND_BASENAME}.md").write_text(
        "---\n"
        'description: "M7.1 mock command"\n'
        "user-invocable: true\n"
        "---\n"
        "\n"
        f"{COMMAND_BODY_MARKER}\n",
        encoding="utf-8",
    )


def case_plugin_install_and_list() -> dict:
    ctx = make_fixture("M7.1")

    plugin_dir = ctx.root_dir / "mock_plugin"
    plugin_dir.mkdir(parents=True, exist_ok=True)
    _build_mock_plugin(plugin_dir)

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { setInlinePlugins } from './bootstrap/state.ts';"
        "import { loadAllPluginsCacheOnly, clearPluginCache } from './utils/plugins/pluginLoader.ts';"
        "import { getPluginCommands, clearPluginCommandCache } from './utils/plugins/loadPluginCommands.ts';"
        f"setInlinePlugins([{json.dumps(str(plugin_dir))}]);"
        "clearPluginCache('M7.1 test setup');"
        "clearPluginCommandCache();"
        "const result = await loadAllPluginsCacheOnly();"
        "const enabled = result.enabled.map(p => ({"
        "  name: p.name,"
        "  source: p.source,"
        "  hasCommandsPath: typeof p.commandsPath === 'string' && p.commandsPath.length > 0,"
        "  commandsPath: p.commandsPath || null,"
        "}));"
        "const cmds = await getPluginCommands();"
        "process.stdout.write(JSON.stringify({"
        "  enabledCount: result.enabled.length,"
        "  enabled,"
        "  errorCount: result.errors.length,"
        "  errors: result.errors.map(e => ({type: e.type, source: e.source})),"
        "  commandCount: cmds.length,"
        "  commandNames: cmds.map(c => c.name),"
        "}) + '\\n');"
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
    )

    write_command_log(
        ctx,
        [RUN_BUN, "-e", "<setInlinePlugins+loadAllPluginsCacheOnly>"],
        proc.stdout,
        proc.stderr,
        proc.returncode,
    )

    parsed = None
    for line in reversed((proc.stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                parsed = json.loads(line)
                break
            except json.JSONDecodeError:
                continue

    if not parsed:
        return {
            "name": "plugin_install_and_list",
            "ok": False,
            "exit_code": proc.returncode,
            "stdout_excerpt": (proc.stdout or "")[:500],
            "stderr_excerpt": (proc.stderr or "")[:500],
            "fixture_root": str(ctx.root_dir),
            "_ctx": ctx,
        }

    enabled_entries = parsed.get("enabled", []) or []
    matching = [e for e in enabled_entries if e.get("name") == PLUGIN_NAME]
    plugin_in_enabled = len(matching) >= 1
    plugin_source_inline = (
        plugin_in_enabled
        and isinstance(matching[0].get("source"), str)
        and "@inline" in matching[0]["source"]
    )
    plugin_commands_path_set = (
        plugin_in_enabled and matching[0].get("hasCommandsPath") is True
    )

    command_names = parsed.get("commandNames", []) or []
    expected_cmd_in_list = EXPECTED_FULL_CMD_NAME in command_names

    return {
        "name": "plugin_install_and_list",
        "ok": (
            proc.returncode == 0
            and plugin_in_enabled
            and plugin_source_inline
            and plugin_commands_path_set
            and expected_cmd_in_list
        ),
        "exit_code": proc.returncode,
        "enabledCount": parsed.get("enabledCount"),
        "plugin_in_enabled": plugin_in_enabled,
        "plugin_source_inline": plugin_source_inline,
        "plugin_commands_path_set": plugin_commands_path_set,
        "matching_plugin": matching[0] if plugin_in_enabled else None,
        "commandCount": parsed.get("commandCount"),
        "expected_cmd_in_list": expected_cmd_in_list,
        "expected_cmd_name": EXPECTED_FULL_CMD_NAME,
        "command_names_preview": command_names[:20],
        "loader_errors": parsed.get("errors", []),
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_plugin_install_and_list()
    ctx = res.pop("_ctx")
    results = [res]

    write_assertions(
        ctx,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {
                "name": r["name"],
                "expected": True,
                "actual": r.get("ok"),
                "passed": r.get("ok"),
                "evidence": (
                    f"plugin_in_enabled={r.get('plugin_in_enabled')} "
                    f"source_inline={r.get('plugin_source_inline')} "
                    f"commandsPath_set={r.get('plugin_commands_path_set')} "
                    f"expected_cmd_in_list={r.get('expected_cmd_in_list')} "
                    f"enabledCount={r.get('enabledCount')} "
                    f"commandCount={r.get('commandCount')}"
                ),
            }
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M7.1 plugin install+list: setInlinePlugins([dir]) → "
            "loadAllPluginsCacheOnly() must include mock_plugin_M7_1@inline; "
            "getPluginCommands() must include 'mock_plugin_M7_1:mock_cmd_M7_1'."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
