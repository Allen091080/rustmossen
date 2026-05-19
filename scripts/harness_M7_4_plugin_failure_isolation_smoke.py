#!/usr/bin/env python3
"""
M7.4 — 坏 plugin 不拖垮主进程, 错误可见且好 plugin 仍工作.

按 harness全链路测试.md §3.7 M7.4 契约:
  前置: fixture root 下两个 inline plugin
    - m74_good: .mossen-plugin/plugin.json + commands/m74_good_cmd.md (合法)
    - m74_bad:  .mossen-plugin/plugin.json (corrupt JSON, 让 loadPluginManifest throw)
  步骤: bun -e setInlinePlugins([good_dir, bad_dir]) → loadAllPluginsCacheOnly()
  观察点:
    1. bun exit 0 (loader 整体不 throw)
    2. result.enabled 含 m74_good
    3. result.errors 含某 entry: source 形如 'inline[N]', type 'generic-error'
       (loadSessionOnlyPlugins:2972 catch 真捕到 createPluginFromPath 的 throw)
    4. getPluginCommands() 含 'm74_good:m74_good_cmd' (好 plugin 命令仍可用)
  反测信号: src/utils/plugins/pluginLoader.ts:loadSessionOnlyPlugins 的 try/catch
            (line 2939-2983) 改成裸 await → bad plugin 让循环抛出 →
            assemblePluginLoadResult 整体 reject → bun exit 非 0 / enabled 空 → fail

  真实导出 (M7.1 已用): setInlinePlugins, loadAllPluginsCacheOnly, getPluginCommands.
  PluginError 形状: {type: 'generic-error', source: 'inline[N]', error: '...'}
  (见 src/utils/plugins/pluginLoader.ts:2978).
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

GOOD_PLUGIN_NAME = "m74_good"
BAD_PLUGIN_NAME = "m74_bad"
GOOD_CMD_BASENAME = "m74_good_cmd"
EXPECTED_GOOD_CMD = f"{GOOD_PLUGIN_NAME}:{GOOD_CMD_BASENAME}"


def _build_good_plugin(plugin_dir: Path) -> None:
    manifest_dir = plugin_dir / ".mossen-plugin"
    manifest_dir.mkdir(parents=True, exist_ok=True)
    (manifest_dir / "plugin.json").write_text(
        json.dumps(
            {
                "name": GOOD_PLUGIN_NAME,
                "version": "0.0.1",
                "description": "M7.4 good fixture plugin",
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    commands_dir = plugin_dir / "commands"
    commands_dir.mkdir(parents=True, exist_ok=True)
    (commands_dir / f"{GOOD_CMD_BASENAME}.md").write_text(
        "---\n"
        'description: "M7.4 good cmd"\n'
        "user-invocable: true\n"
        "---\n"
        "\n"
        "M7_4_GOOD_BODY\n",
        encoding="utf-8",
    )


def _build_bad_plugin(plugin_dir: Path) -> None:
    """坏 plugin: plugin.json 内容是非法 JSON, 触发 loadPluginManifest 的 corrupt-manifest throw."""
    manifest_dir = plugin_dir / ".mossen-plugin"
    manifest_dir.mkdir(parents=True, exist_ok=True)
    # 故意写非法 JSON: 没有引号 + 缺括号
    (manifest_dir / "plugin.json").write_text(
        "{ this is not valid json at all (((( M7_4_BAD",
        encoding="utf-8",
    )


def case_bad_plugin_isolated_good_works() -> dict:
    ctx = make_fixture("M7.4")

    good_dir = ctx.root_dir / "good_plugin"
    bad_dir = ctx.root_dir / "bad_plugin"
    good_dir.mkdir(parents=True, exist_ok=True)
    bad_dir.mkdir(parents=True, exist_ok=True)
    _build_good_plugin(good_dir)
    _build_bad_plugin(bad_dir)

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { setInlinePlugins } from './bootstrap/state.ts';"
        "import { loadAllPluginsCacheOnly, clearPluginCache } from './utils/plugins/pluginLoader.ts';"
        "import { getPluginCommands, clearPluginCommandCache } from './utils/plugins/loadPluginCommands.ts';"
        f"setInlinePlugins({json.dumps([str(good_dir), str(bad_dir)])});"
        "clearPluginCache('M7.4 test setup');"
        "clearPluginCommandCache();"
        "let loaderThrew = false;"
        "let result = null;"
        "try {"
        "  result = await loadAllPluginsCacheOnly();"
        "} catch (e) {"
        "  loaderThrew = String(e && e.stack || e);"
        "}"
        "const enabled = (result && result.enabled || []).map(p => ({"
        "  name: p.name, source: p.source,"
        "}));"
        "const errors = (result && result.errors || []).map(e => ({"
        "  type: e.type, source: e.source,"
        "  message: typeof e.error === 'string' ? e.error.slice(0, 200) : null,"
        "}));"
        "const cmds = await getPluginCommands();"
        "process.stdout.write(JSON.stringify({"
        "  loaderThrew,"
        "  enabledCount: enabled.length,"
        "  enabled,"
        "  errorCount: errors.length,"
        "  errors,"
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
        [RUN_BUN, "-e", "<setInlinePlugins([good, bad]) + loadAllPluginsCacheOnly>"],
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
            "name": "bad_plugin_isolated_good_works",
            "ok": False,
            "exit_code": proc.returncode,
            "stdout_excerpt": (proc.stdout or "")[:500],
            "stderr_excerpt": (proc.stderr or "")[:500],
            "fixture_root": str(ctx.root_dir),
            "_ctx": ctx,
        }

    enabled_entries = parsed.get("enabled") or []
    errors_entries = parsed.get("errors") or []
    command_names = parsed.get("commandNames") or []

    bun_exit_zero = proc.returncode == 0
    loader_did_not_throw = parsed.get("loaderThrew") in (False, None)
    good_in_enabled = any(e.get("name") == GOOD_PLUGIN_NAME for e in enabled_entries)
    # bad plugin 应在 errors 里 — 至少有一条 inline[N] 来源的 generic-error / corrupt-manifest
    bad_error_visible = any(
        (e.get("source") or "").startswith("inline[")
        and (
            e.get("type") in ("generic-error", "manifest-error", "manifest-invalid")
            or "M7_4_BAD" in (e.get("message") or "")
            or "corrupt" in (e.get("message") or "").lower()
            or "manifest" in (e.get("message") or "").lower()
            or "json" in (e.get("message") or "").lower()
        )
        for e in errors_entries
    )
    good_cmd_in_list = EXPECTED_GOOD_CMD in command_names

    return {
        "name": "bad_plugin_isolated_good_works",
        "ok": (
            bun_exit_zero
            and loader_did_not_throw
            and good_in_enabled
            and bad_error_visible
            and good_cmd_in_list
        ),
        "exit_code": proc.returncode,
        "loader_did_not_throw": loader_did_not_throw,
        "loader_throw_msg": parsed.get("loaderThrew"),
        "good_in_enabled": good_in_enabled,
        "bad_error_visible": bad_error_visible,
        "good_cmd_in_list": good_cmd_in_list,
        "enabled_count": parsed.get("enabledCount"),
        "enabled_names": [e.get("name") for e in enabled_entries],
        "error_count": parsed.get("errorCount"),
        "errors_preview": errors_entries[:5],
        "command_names_preview": command_names[:20],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_bad_plugin_isolated_good_works()
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
                    f"loader_no_throw={r.get('loader_did_not_throw')} "
                    f"good_enabled={r.get('good_in_enabled')} "
                    f"bad_error_visible={r.get('bad_error_visible')} "
                    f"good_cmd={r.get('good_cmd_in_list')}"
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
            "M7.4 failure isolation: setInlinePlugins([good, bad-corrupt-json]) → "
            "loadAllPluginsCacheOnly resolves; result.enabled has m74_good; "
            "result.errors has inline[N] generic-error for m74_bad; "
            "getPluginCommands lists 'm74_good:m74_good_cmd'."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
