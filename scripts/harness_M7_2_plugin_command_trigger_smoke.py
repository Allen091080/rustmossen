#!/usr/bin/env python3
"""
M7.2 — plugin command 真触发 + body 还原 (PLUGIN_M7_2_RAN marker).

按 harness全链路测试.md §3.7 M7.2 契约:
  前置: 同 M7.1 mock plugin (plugin.json + commands/mock_cmd_M7_1.md
        body 含 marker "PLUGIN_M7_2_RAN")
  步骤: setInlinePlugins → getPluginCommands() 取 'mock_plugin_M7_1:mock_cmd_M7_1' →
        cmd.getPromptForCommand("", stubContext) 真展开 prompt
  观察点:
    1. cmd 真在 list 里 (上游真注册了 plugin command)
    2. cmd.type === 'prompt' 且 cmd.source === 'plugin'
    3. cmd.userInvocable === true (frontmatter 真被解析)
    4. cmd.contentLength > 0
    5. getPromptForCommand 返回的 ContentBlock 含 PLUGIN_M7_2_RAN
       (确认 body 真被作为 prompt 内容下发, 不是空 stub)
  反测信号: 改 loadCommandsFromDirectory 跳过 commands/ 目录 →
            getPluginCommands 不返回此 cmd → cmd undefined → fail
            或 把 frontmatter user-invocable: false → cmd.userInvocable === false → fail

实现策略 (方式 C):
  bun -e snippet 走真 plugin loader 链路, 用 stub ToolUseContext 调
  getPromptForCommand. body 内无 !`...` 或 ```! 块, executeShellCommandsInPrompt
  会跳过 shell 执行分支, context 实际不被访问 (见 promptShellExecution.ts:69-91).
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
COMMAND_BODY_MARKER = "PLUGIN_M7_2_RAN"


def _build_mock_plugin(plugin_dir: Path) -> None:
    manifest_dir = plugin_dir / ".mossen-plugin"
    manifest_dir.mkdir(parents=True, exist_ok=True)
    (manifest_dir / "plugin.json").write_text(
        json.dumps(
            {
                "name": PLUGIN_NAME,
                "version": "0.0.1",
                "description": "M7.2 fixture test plugin",
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    commands_dir = plugin_dir / "commands"
    commands_dir.mkdir(parents=True, exist_ok=True)
    (commands_dir / f"{COMMAND_BASENAME}.md").write_text(
        "---\n"
        'description: "M7.2 mock command body marker"\n'
        "user-invocable: true\n"
        "---\n"
        "\n"
        f"{COMMAND_BODY_MARKER}\n",
        encoding="utf-8",
    )


def case_plugin_command_trigger_real() -> dict:
    ctx = make_fixture("M7.2")

    plugin_dir = ctx.root_dir / "mock_plugin"
    plugin_dir.mkdir(parents=True, exist_ok=True)
    _build_mock_plugin(plugin_dir)

    # bun snippet:
    #   1. 装 inline plugin
    #   2. enumerate plugin commands
    #   3. find target by name
    #   4. invoke getPromptForCommand("", stubCtx)
    #   5. concat ContentBlock text 字段 → 找 marker
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { setInlinePlugins } from './bootstrap/state.ts';"
        "import { clearPluginCache } from './utils/plugins/pluginLoader.ts';"
        "import { getPluginCommands, clearPluginCommandCache } from './utils/plugins/loadPluginCommands.ts';"
        f"setInlinePlugins([{json.dumps(str(plugin_dir))}]);"
        "clearPluginCache('M7.2 test setup');"
        "clearPluginCommandCache();"
        "const cmds = await getPluginCommands();"
        f"const target = cmds.find(c => c.name === {json.dumps(EXPECTED_FULL_CMD_NAME)});"
        "if (!target) {"
        "  process.stdout.write(JSON.stringify({"
        "    found: false,"
        "    commandNames: cmds.map(c => c.name),"
        "  }) + '\\n');"
        "} else {"
        "  let promptBlocks = null;"
        "  let invokeError = null;"
        "  try {"
        # stub ToolUseContext: body 无 shell 块, executeShellCommandsInPrompt 不会真用 context
        "    const stubCtx = {"
        "      abortController: new AbortController(),"
        "      options: {},"
        "      readFileTimestamps: {},"
        "      setToolJSX: () => {},"
        "      getAppState: () => ({ toolPermissionContext: { alwaysAllowRules: {} } }),"
        "    };"
        "    promptBlocks = await target.getPromptForCommand('', stubCtx);"
        "  } catch (e) {"
        "    invokeError = String(e && e.stack || e);"
        "  }"
        "  const promptText = Array.isArray(promptBlocks)"
        "    ? promptBlocks.map(b => (b && typeof b.text === 'string') ? b.text : '').join('\\n')"
        "    : '';"
        "  process.stdout.write(JSON.stringify({"
        "    found: true,"
        "    commandName: target.name,"
        "    type: target.type,"
        "    source: target.source,"
        "    userInvocable: target.userInvocable,"
        "    contentLength: target.contentLength,"
        "    promptBlockCount: Array.isArray(promptBlocks) ? promptBlocks.length : 0,"
        "    promptHasMarker: promptText.includes(" + json.dumps(COMMAND_BODY_MARKER) + "),"
        "    promptExcerpt: promptText.slice(0, 300),"
        "    invokeError,"
        "  }) + '\\n');"
        "}"
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
        [RUN_BUN, "-e", "<plugin-cmd-trigger>"],
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
            "name": "plugin_command_trigger_real",
            "ok": False,
            "exit_code": proc.returncode,
            "stdout_excerpt": (proc.stdout or "")[:500],
            "stderr_excerpt": (proc.stderr or "")[:500],
            "fixture_root": str(ctx.root_dir),
            "_ctx": ctx,
        }

    found = parsed.get("found") is True
    type_ok = parsed.get("type") == "prompt"
    source_ok = parsed.get("source") == "plugin"
    user_invocable_ok = parsed.get("userInvocable") is True
    content_length_ok = (parsed.get("contentLength") or 0) > 0
    prompt_block_ok = (parsed.get("promptBlockCount") or 0) >= 1
    prompt_marker_ok = parsed.get("promptHasMarker") is True
    no_invoke_error = not parsed.get("invokeError")

    return {
        "name": "plugin_command_trigger_real",
        "ok": (
            proc.returncode == 0
            and found
            and type_ok
            and source_ok
            and user_invocable_ok
            and content_length_ok
            and prompt_block_ok
            and prompt_marker_ok
            and no_invoke_error
        ),
        "exit_code": proc.returncode,
        "found": found,
        "type": parsed.get("type"),
        "source": parsed.get("source"),
        "userInvocable": parsed.get("userInvocable"),
        "contentLength": parsed.get("contentLength"),
        "promptBlockCount": parsed.get("promptBlockCount"),
        "promptHasMarker": prompt_marker_ok,
        "promptExcerpt": parsed.get("promptExcerpt"),
        "invokeError": parsed.get("invokeError"),
        "expected_cmd_name": EXPECTED_FULL_CMD_NAME,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_plugin_command_trigger_real()
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
                    f"found={r.get('found')} "
                    f"type={r.get('type')} "
                    f"source={r.get('source')} "
                    f"userInvocable={r.get('userInvocable')} "
                    f"contentLength={r.get('contentLength')} "
                    f"promptBlockCount={r.get('promptBlockCount')} "
                    f"promptHasMarker={r.get('promptHasMarker')}"
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
            "M7.2 plugin command trigger: setInlinePlugins → getPluginCommands → "
            "find('mock_plugin_M7_1:mock_cmd_M7_1') → getPromptForCommand returns "
            "ContentBlock containing 'PLUGIN_M7_2_RAN' body marker."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
