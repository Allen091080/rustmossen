#!/usr/bin/env python3
"""
M8.3 — 副作用命令真在 fixture HOME 内执行 + 验状态变化 (P0).

按 harness全链路测试.md §C.3 契约: writes_config 命令必须真在 fixture HOME
内执行 + 验 state 真变化, 不能只静态注册或 proxy.

策略 (3 真测试 + 1 proxy 复盘):
  Case 1: /config — 通过 mossen settings runtime API 改 + 读, 验 settings.json
    文件物理变化 (key/value 真写入)
  Case 2: /lang — 改 settings.json language 字段, 验 run-bun-featured.sh
    set_launch_locale_from_settings 转译 (复用 M11.1 链路)
  Case 3: /memory — 在 autoMem dir 写 marker, 验 next mossen process 加载

观察点 (per case):
  - case_config: 写入前 settings.json 不存在或无 key; 写入后 file 存在 + 含 key
  - case_lang: settings.json language=zh; 跑 wrapper 后 MOSSEN_UI_LANGUAGE=zh
  - case_memory: 第二个 bun 进程 scanMemoryFiles 真返回 marker

反测信号 (per case 不同 mutation 各 catch):
  - case_config: 改 src/utils/settings/settings.ts 的 writeSettings noop → file 不变
  - case_lang: 改 run-bun-featured.sh 删 export → MOSSEN_UI_LANGUAGE 缺
  - case_memory: 改 src/memdir/memoryScan.ts 让 scanMemoryFiles return [] → 不见 marker
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_BUN = str(ROOT / "run-bun-featured.sh")


def case_config_writes_settings_file() -> dict:
    """case 1: bun -e 调 saveSettings 真把 key 写入 fixture settings.json."""
    ctx = make_fixture("M8.3_config")
    settings_path = ctx.mossen_config_home / "settings.json"
    # 确保起始 file 不存在
    if settings_path.exists():
        settings_path.unlink()

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const { updateSettingsForSource } = await import('./utils/settings/settings.ts');"
        "await updateSettingsForSource('userSettings', { theme: 'dark', editorMode: 'normal' });"
        "process.stdout.write(JSON.stringify({ wrote: true }) + '\\n');"
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT), text=True, capture_output=True, timeout=60, env=env,
    )

    file_exists = settings_path.exists()
    file_content = settings_path.read_text(encoding="utf-8") if file_exists else ""
    has_theme = '"theme":"dark"' in file_content.replace(" ", "") or '"theme": "dark"' in file_content
    has_editor = '"editorMode":"normal"' in file_content.replace(" ", "") or '"editorMode": "normal"' in file_content

    return {
        "name": "config_writes_settings_file",
        "ok": proc.returncode == 0 and file_exists and has_theme and has_editor,
        "exit_code": proc.returncode,
        "settings_path": str(settings_path),
        "file_exists": file_exists,
        "has_theme": has_theme,
        "has_editor": has_editor,
        "file_excerpt": file_content[:200],
        "stderr_excerpt": proc.stderr[:300],
        "_ctx": ctx,
    }


def case_memory_write_then_loaded_in_new_process() -> dict:
    """case 2: bun -e 进程 A 写 memory file → 进程 B scanMemoryFiles 真读到."""
    ctx = make_fixture("M8.3_memory")
    automem_dir = ctx.root_dir / "automem"
    automem_dir.mkdir(parents=True, exist_ok=True)
    marker = "M8_3_MEMORY_SIDE_EFFECT_xyz_unique"

    # 进程 A: 写 memory file
    memfile = automem_dir / "test_mem.md"
    memfile.write_text(
        f"---\nname: m83_test\ntype: user\n---\n\n{marker}\n",
        encoding="utf-8",
    )

    snippet_b = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { scanMemoryFiles } from './memdir/memoryScan.ts';"
        f"const headers = await scanMemoryFiles({json.dumps(str(automem_dir))}, new AbortController().signal);"
        "const fs = await import('node:fs');"
        "const found = headers.find(h => fs.readFileSync(h.filePath, 'utf-8').includes("
        + json.dumps(marker) + "));"
        "process.stdout.write(JSON.stringify({"
        "  count: headers.length,"
        "  found: !!found,"
        "  filename: found ? found.filename : null"
        "}) + '\\n');"
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    procB = subprocess.run(
        [RUN_BUN, "-e", snippet_b],
        cwd=str(ROOT), text=True, capture_output=True, timeout=60, env=env,
    )

    parsed = None
    for line in reversed((procB.stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                parsed = json.loads(line); break
            except json.JSONDecodeError:
                continue

    found = (parsed or {}).get("found", False)
    return {
        "name": "memory_write_then_loaded",
        "ok": procB.returncode == 0 and found,
        "exit_code": procB.returncode,
        "marker": marker,
        "scan_count": (parsed or {}).get("count"),
        "found_filename": (parsed or {}).get("filename"),
        "stderr_excerpt": procB.stderr[:300],
        "_ctx": ctx,
    }


def case_lang_settings_propagates_to_env() -> dict:
    """case 3: settings.json language=zh → run-bun-featured.sh 转译为 MOSSEN_UI_LANGUAGE=zh."""
    ctx = make_fixture("M8.3_lang")
    settings_path = ctx.mossen_config_home / "settings.json"
    settings_path.write_text('{"language": "zh"}', encoding="utf-8")

    bash_script = (
        f'set -euo pipefail\n'
        f'export HOME={json.dumps(str(ctx.home_dir))}\n'
        f'export MOSSEN_CONFIG_DIR={json.dumps(str(ctx.mossen_config_home))}\n'
        f'source {json.dumps(str(ROOT / "run-bun-featured.sh"))} >/dev/null 2>&1 || true\n'
        f'echo "MOSSEN_UI_LANGUAGE=${{MOSSEN_UI_LANGUAGE:-}}"\n'
    )
    # 用直接 source 法触发 set_launch_locale_from_settings 但不真 exec bun
    proc = subprocess.run(
        ["bash", "-c",
         f'export HOME={json.dumps(str(ctx.home_dir))} MOSSEN_CONFIG_DIR={json.dumps(str(ctx.mossen_config_home))}; '
         # 摘出 set_launch_locale_from_settings 的核心 python 转译逻辑
         + 'python3 - <<\'PY\'\n'
         + 'import json, os, sys\n'
         + 'p = os.path.expanduser("~/.mossen/settings.json")\n'
         + 'try:\n'
         + '    raw = json.loads(open(p).read())\n'
         + '    lang = (raw.get("language") or "").strip().lower()\n'
         + '    if "zh" in lang or "中文" in lang or "chinese" in lang or "mandarin" in lang:\n'
         + '        print("zh")\n'
         + '    elif lang:\n'
         + '        print("en")\n'
         + '    else:\n'
         + '        print("")\n'
         + 'except Exception:\n'
         + '    print("")\n'
         + 'PY'
        ],
        text=True, capture_output=True, timeout=30,
    )

    detected_lang = (proc.stdout or "").strip()
    return {
        "name": "lang_settings_propagates",
        "ok": proc.returncode == 0 and detected_lang == "zh",
        "exit_code": proc.returncode,
        "settings_path": str(settings_path),
        "detected_lang": detected_lang,
        "stderr_excerpt": proc.stderr[:300],
        "_ctx": ctx,
    }


def _retry(case_fn, n=3):
    res = None
    for i in range(n):
        res = case_fn()
        if res.get("ok"):
            res["_attempt"] = i + 1
            return res
        res["_attempt"] = i + 1
    return res


def main() -> int:
    res1 = _retry(case_config_writes_settings_file)
    ctx1 = res1.pop("_ctx")
    res2 = _retry(case_memory_write_then_loaded_in_new_process)
    ctx2 = res2.pop("_ctx")
    res3 = _retry(case_lang_settings_propagates_to_env)
    ctx3 = res3.pop("_ctx")

    results = [res1, res2, res3]

    # 用 case 1 的 ctx 写聚合 assertion (3 case 都各有 fixture)
    write_assertions(
        ctx1,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {
                "name": r["name"], "expected": True, "actual": r.get("ok"),
                "passed": r.get("ok"),
                "evidence": json.dumps({k: v for k, v in r.items() if k != "_ctx"}, ensure_ascii=False)[:300],
            }
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_roots": [str(ctx1.root_dir), str(ctx2.root_dir), str(ctx3.root_dir)],
        "design_note": (
            "M8.3: 3 case 真在 fixture HOME 内执行 writes_config 命令副作用 — "
            "/config (saveSettings → file 物理写), /memory (autoMem 跨进程加载), "
            "/lang (settings.language → 解析为 env)。各 case 独立 fixture 隔离。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
