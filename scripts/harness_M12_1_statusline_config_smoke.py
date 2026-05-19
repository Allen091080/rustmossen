#!/usr/bin/env python3
"""
M12.1 — 自定义 statusline 脚本能渲染 model/ctx/cwd/lang。

按 harness全链路测试.md §C.1 (M12.1 P0) 契约:
  前置:
    - fixture HOME 隔离 + MOSSEN_CONFIG_DIR 显式覆盖
    - userSettings (~/.mossen/settings.json) 写入 statusLine.command 指向自写脚本
    - 自写 statusline 脚本: 读 stdin JSON payload → 打印含 unique marker
      + model.id + workspace.current_dir 字面
  步骤:
    bun -e 调 executeStatusLineCommand(payload), 拿 stdout
  观察点 (强契约):
    1. bun 进程 exit 0
    2. stdout 含 unique marker  STATUSLINE_M12_1_CUSTOM_xyz
    3. stdout 含 payload.model.id 字面
    4. stdout 含 payload.workspace.current_dir 字面
    5. stdout 含 payload.interactive_language 字面 (lang)
  反测信号:
    - 改 src/utils/hooks.ts:executeStatusLineCommand 让它永返 undefined
      → marker 不在 → fail
    - 改 statusline 脚本不输出 marker → 观察点 2 fail
    - 删 settings.json statusLine 配置 → output undefined → fail
"""

from __future__ import annotations

import json
import os
import stat
import subprocess
import sys
import textwrap
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

MARKER = "STATUSLINE_M12_1_CUSTOM_xyz_unique"
MODEL_ID = "claude-sonnet-4-5-mossen-test"
LANG_VALUE = "中文"


def case_statusline_custom_command() -> dict:
    ctx = make_fixture("M12.1")

    # 子进程读 MOSSEN_CONFIG_DIR (不是 MOSSEN_CONFIG_HOME)
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    # 自写 statusline 脚本: 输出 marker + payload 关键字段
    script_path = ctx.root_dir / "statusline_M12_1.sh"
    script_path.write_text(
        "#!/usr/bin/env bash\n"
        f"PAYLOAD=$(cat)\n"
        # 用 python 解析 JSON, 比 bash 解析更可靠
        "python3 -c \"\n"
        "import json, sys\n"
        "p = json.loads(sys.stdin.read())\n"
        f"marker = '{MARKER}'\n"
        "model_id = (p.get('model') or {}).get('id', 'NO_MODEL')\n"
        "cwd = (p.get('workspace') or {}).get('current_dir', 'NO_CWD')\n"
        "lang = p.get('interactive_language', 'NO_LANG')\n"
        "print(f'{marker} | model={model_id} | cwd={cwd} | lang={lang}')\n"
        "\" <<< \"$PAYLOAD\"\n"
    )
    script_path.chmod(0o755)

    # 写 user settings.json (在 MOSSEN_CONFIG_DIR / settings.json)
    settings_path = ctx.mossen_config_home / "settings.json"
    settings_payload = {
        "statusLine": {
            "type": "command",
            "command": str(script_path),
            "padding": 0,
        },
    }
    settings_path.parent.mkdir(parents=True, exist_ok=True)
    settings_path.write_text(json.dumps(settings_payload, indent=2))

    # 用 bun -e 直接调 executeStatusLineCommand
    cwd_for_test = str(ctx.root_dir)
    probe_script = textwrap.dedent(
        f"""\
        import {{ setSessionTrustAccepted }} from './bootstrap/state.ts'
        import {{ executeStatusLineCommand }} from './utils/hooks.ts'

        // statusLine 默认要求 trust accepted, 否则被 skip
        setSessionTrustAccepted(true)

        const payload = {{
          session_id: 'harness-M12-1-statusline',
          transcript_path: '/tmp/harness-M12-1.jsonl',
          cwd: {json.dumps(cwd_for_test)},
          model: {{
            id: {json.dumps(MODEL_ID)},
            display_name: {json.dumps(MODEL_ID)},
          }},
          workspace: {{
            current_dir: {json.dumps(cwd_for_test)},
            project_dir: {json.dumps(cwd_for_test)},
            added_dirs: [],
          }},
          version: 'harness-M12-1',
          output_style: {{ name: 'default' }},
          interactive_language: {json.dumps(LANG_VALUE)},
          context_window: {{
            total_input_tokens: 0,
            total_output_tokens: 0,
            context_window_size: 200000,
            current_usage: null,
            used_percentage: 0,
            remaining_percentage: 100,
          }},
          exceeds_200k_tokens: false,
        }}
        const output = await executeStatusLineCommand(payload)
        if (output === undefined) {{
          console.error('STATUSLINE_RETURNED_UNDEFINED')
          process.exit(2)
        }}
        process.stdout.write(output + '\\n')
        """
    )

    proc = subprocess.run(
        [str(ROOT / "run-bun-featured.sh"), "-e", probe_script],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
        cwd=str(ROOT),
    )

    write_command_log(
        ctx,
        ["bun", "-e", "<probe-statusline>"],
        proc.stdout,
        proc.stderr,
        proc.returncode,
    )

    marker_in_stdout = MARKER in proc.stdout
    model_in_stdout = MODEL_ID in proc.stdout
    cwd_in_stdout = cwd_for_test in proc.stdout
    lang_in_stdout = LANG_VALUE in proc.stdout

    ok = (
        proc.returncode == 0
        and marker_in_stdout
        and model_in_stdout
        and cwd_in_stdout
        and lang_in_stdout
    )

    return {
        "name": "statusline_custom_command_render",
        "ok": ok,
        "exit_code": proc.returncode,
        "marker_in_stdout": marker_in_stdout,
        "model_id_in_stdout": model_in_stdout,
        "cwd_in_stdout": cwd_in_stdout,
        "lang_in_stdout": lang_in_stdout,
        "settings_path": str(settings_path),
        "script_path": str(script_path),
        "stdout_excerpt": proc.stdout[:400],
        "stderr_excerpt": proc.stderr[:400],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_statusline_custom_command()
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
                    f"marker={r.get('marker_in_stdout')} "
                    f"model={r.get('model_id_in_stdout')} "
                    f"cwd={r.get('cwd_in_stdout')} "
                    f"lang={r.get('lang_in_stdout')} "
                    f"exit={r.get('exit_code')}"
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
            "M12.1 自定义 statusline: bun -e 调 executeStatusLineCommand "
            "+ user settings.json 配置 + 自写脚本输出 marker → 验 marker/model/cwd/lang 全在"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
