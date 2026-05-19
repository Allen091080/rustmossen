#!/usr/bin/env python3
"""
M7.3 — plugin reload 后新 cmd 可见, disable (setInlinePlugins([])) 后不可触发.

按 harness全链路测试.md §3.7 M7.3 契约:
  前置: 装 1 个 inline plugin (commands/cmd_v1.md)
  步骤 (3 phases, 3 个 bun -e 子进程, 模拟 reload + disable):
    A. setInlinePlugins([dir]) → loadAllPluginsCacheOnly → getPluginCommands
       验: cmd_v1 in commands
    B. python 在 commands/ 下追加 cmd_v2.md
       新 bun -e: clearPluginCache + clearPluginCommandCache + setInlinePlugins
                  → loadAllPluginsCacheOnly → getPluginCommands
       验: cmd_v1 AND cmd_v2 都 in commands (reload 真生效)
    C. 新 bun -e: setInlinePlugins([]) → clearPluginCache + clearPluginCommandCache
                  → loadAllPluginsCacheOnly → getPluginCommands
       验: cmd_v1 AND cmd_v2 都 NOT in commands (disable 真生效, plugin 不在 enabled)
  反测信号: src/utils/plugins/pluginLoader.ts:clearPluginCache 改成 noop →
            phase B 进程仍命中老 cache → 只看到 v1 → fail

  注: 每个 bun -e 是独立 process, memoize 缓存自然不共享; clearPluginCache 在同
      process 内才相关. phase C 的 disable 验的是: setInlinePlugins([]) 后
      getInlinePlugins() 返回 [], loadSessionOnlyPlugins 不跑, 故 plugin 不再 enabled.
      由于 phase C 不写真 marketplace install, 该 plugin 也不会从 marketplace 来.

  真实导出 (M7.1/M7.2 已用):
    setInlinePlugins, getInlinePlugins  ← src/bootstrap/state.ts
    loadAllPluginsCacheOnly, clearPluginCache ← src/utils/plugins/pluginLoader.ts
    getPluginCommands, clearPluginCommandCache ← src/utils/plugins/loadPluginCommands.ts
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

PLUGIN_NAME = "mock_plugin_M7_3"
CMD_V1 = "cmd_v1_M7_3"
CMD_V2 = "cmd_v2_M7_3"
EXPECTED_V1 = f"{PLUGIN_NAME}:{CMD_V1}"
EXPECTED_V2 = f"{PLUGIN_NAME}:{CMD_V2}"


def _build_mock_plugin_v1(plugin_dir: Path) -> None:
    manifest_dir = plugin_dir / ".mossen-plugin"
    manifest_dir.mkdir(parents=True, exist_ok=True)
    (manifest_dir / "plugin.json").write_text(
        json.dumps(
            {
                "name": PLUGIN_NAME,
                "version": "0.0.1",
                "description": "M7.3 reload/disable fixture plugin",
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    commands_dir = plugin_dir / "commands"
    commands_dir.mkdir(parents=True, exist_ok=True)
    (commands_dir / f"{CMD_V1}.md").write_text(
        "---\n"
        'description: "M7.3 v1"\n'
        "user-invocable: true\n"
        "---\n"
        "\n"
        "M7_3_V1_BODY\n",
        encoding="utf-8",
    )


def _add_command_v2(plugin_dir: Path) -> None:
    commands_dir = plugin_dir / "commands"
    (commands_dir / f"{CMD_V2}.md").write_text(
        "---\n"
        'description: "M7.3 v2 added after reload"\n'
        "user-invocable: true\n"
        "---\n"
        "\n"
        "M7_3_V2_BODY\n",
        encoding="utf-8",
    )


def _run_bun_phase(env: dict, plugin_dir_for_setInline: list[str], label: str) -> dict:
    """运行一次 bun -e: setInlinePlugins(plugin_dir_for_setInline) → 取 commandNames."""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { setInlinePlugins } from './bootstrap/state.ts';"
        "import { loadAllPluginsCacheOnly, clearPluginCache } from './utils/plugins/pluginLoader.ts';"
        "import { getPluginCommands, clearPluginCommandCache } from './utils/plugins/loadPluginCommands.ts';"
        f"setInlinePlugins({json.dumps(plugin_dir_for_setInline)});"
        f"clearPluginCache({json.dumps('M7.3 ' + label)});"
        "clearPluginCommandCache();"
        "const result = await loadAllPluginsCacheOnly();"
        "const cmds = await getPluginCommands();"
        "process.stdout.write(JSON.stringify({"
        "  enabledNames: result.enabled.map(p => p.name),"
        "  commandNames: cmds.map(c => c.name),"
        "  errorCount: result.errors.length,"
        "  errors: result.errors.map(e => ({type: e.type, source: e.source})),"
        "}) + '\\n');"
    )

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
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

    return {
        "label": label,
        "exit_code": proc.returncode,
        "parsed": parsed,
        "stdout": proc.stdout or "",
        "stderr": proc.stderr or "",
    }


def case_plugin_reload_and_disable() -> dict:
    ctx = make_fixture("M7.3")

    plugin_dir = ctx.root_dir / "mock_plugin"
    plugin_dir.mkdir(parents=True, exist_ok=True)
    _build_mock_plugin_v1(plugin_dir)

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    # Phase A: 装 plugin, 验只看到 v1
    phase_a = _run_bun_phase(env, [str(plugin_dir)], "phaseA-initial")

    # Phase B: 加 v2 命令, 重新 setInlinePlugins, 验 v1 + v2 都见
    _add_command_v2(plugin_dir)
    phase_b = _run_bun_phase(env, [str(plugin_dir)], "phaseB-reload")

    # Phase C: setInlinePlugins([]) (disable inline source), 验 v1/v2 都不见
    phase_c = _run_bun_phase(env, [], "phaseC-disable")

    # 写主进程 command log (只用 phase A 作主 stdout 证据)
    write_command_log(
        ctx,
        [RUN_BUN, "-e", "<3-phase reload/disable>"],
        f"=== A ===\n{phase_a['stdout']}\n=== B ===\n{phase_b['stdout']}\n=== C ===\n{phase_c['stdout']}",
        f"=== A ===\n{phase_a['stderr']}\n=== B ===\n{phase_b['stderr']}\n=== C ===\n{phase_c['stderr']}",
        phase_a["exit_code"] | phase_b["exit_code"] | phase_c["exit_code"],
    )

    a_ok = (
        phase_a["exit_code"] == 0
        and phase_a["parsed"] is not None
        and EXPECTED_V1 in (phase_a["parsed"].get("commandNames") or [])
    )
    b_ok = (
        phase_b["exit_code"] == 0
        and phase_b["parsed"] is not None
        and EXPECTED_V1 in (phase_b["parsed"].get("commandNames") or [])
        and EXPECTED_V2 in (phase_b["parsed"].get("commandNames") or [])
    )
    c_names = (phase_c["parsed"] or {}).get("commandNames") or []
    c_enabled = (phase_c["parsed"] or {}).get("enabledNames") or []
    c_ok = (
        phase_c["exit_code"] == 0
        and phase_c["parsed"] is not None
        and EXPECTED_V1 not in c_names
        and EXPECTED_V2 not in c_names
        and PLUGIN_NAME not in c_enabled
    )

    return {
        "name": "plugin_reload_and_disable",
        "ok": a_ok and b_ok and c_ok,
        "phase_a_ok": a_ok,
        "phase_b_ok": b_ok,
        "phase_c_ok": c_ok,
        "phase_a_exit": phase_a["exit_code"],
        "phase_b_exit": phase_b["exit_code"],
        "phase_c_exit": phase_c["exit_code"],
        "phase_a_commands": (phase_a["parsed"] or {}).get("commandNames"),
        "phase_b_commands": (phase_b["parsed"] or {}).get("commandNames"),
        "phase_c_commands": c_names,
        "phase_c_enabled": c_enabled,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_plugin_reload_and_disable()
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
                    f"phaseA_v1_visible={r.get('phase_a_ok')} "
                    f"phaseB_v1+v2_visible={r.get('phase_b_ok')} "
                    f"phaseC_disabled={r.get('phase_c_ok')}"
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
            "M7.3 reload+disable: A=v1, add v2.md, B=v1+v2 (reload via "
            "clearPluginCache), C=setInlinePlugins([]) → plugin removed from "
            "enabled & both commands gone."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
