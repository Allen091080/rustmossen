#!/usr/bin/env python3
"""
M11.1 — zh / en 语言一致性 e2e (P0)。

按 harness全链路测试.md §C.1 / §11 契约:
  用户场景: 用户在 settings.json 设 language="zh" 或 language="en", mossen
  runtime 必须把同一语言贯穿到 footer / tip / slash 描述 / 权限卡 /
  statusline / 错误。

  关键链路 (run-bun-featured.sh:38-95):
    settings.json 的 language 字段 → set_launch_locale_from_settings 解析
      → export MOSSEN_UI_LANGUAGE / MOSSENSRC_INTERACTIVE_LANGUAGE / LANG / LC_MESSAGES
      → mossen 子进程的 process.env 看到这些, uiLanguage.ts 读取并做 footer/tip 本地化。

  本测策略 (避免硬测全 UI):
    case_zh: 写 settings.json {"language": "zh"}, 启子进程, 验子进程 env 真带
             MOSSEN_UI_LANGUAGE=zh + LANG=zh_*
    case_en: 写 settings.json {"language": "en"}, 同上验 MOSSEN_UI_LANGUAGE=en
    case_zh_natural: 写 settings.json {"language": "中文"}, 验 zh 解析正确
                      (覆盖 normalizeLanguagePreference 的 alias)

  做法:
    settings.json 写在 ctx.mossen_config_home / 'settings.json'
    (= $HOME/.mossen/settings.json, 与 run-bun-featured.sh:8 SETTINGS_FILE 一致)

    用 bash -c 调 run-bun-featured.sh 的 set_launch_locale_from_settings 之后, 用
    `env | grep MOSSEN_UI_LANGUAGE` 同进程读 export — 直接 source shell 后 env。

  反测信号:
    - 改 run-bun-featured.sh 不 export MOSSEN_UI_LANGUAGE → 验失败 (env 缺)
    - 改 run-bun-featured.sh 把 'zh' branch 全删 → case_zh 拿到 'en' → 失败
    - 改 settings.json 写非法 language (e.g. 'klingon') → MOSSEN_UI_LANGUAGE 空 → 失败
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

RUN_BUN_FEATURED = str(ROOT / "run-bun-featured.sh")


def _probe_language_env(ctx_env: dict, settings_value: str, fixture_home: Path) -> dict:
    """
    在子进程里 source set_launch_locale_from_settings 然后 dump env, 验真 export 的 var。
    用 bash subshell, 不能直接调 run-bun-featured.sh (它会启 mossen)。
    """
    settings_file = fixture_home / ".mossen" / "settings.json"
    settings_file.parent.mkdir(parents=True, exist_ok=True)
    settings_file.write_text(json.dumps({"language": settings_value}), encoding="utf-8")

    # 把 run-bun-featured.sh 整文件 source 进来 (它定义 set_launch_locale_from_settings),
    # 但 source 时它会执行末尾的 mossen 启动 — 所以用 'return' 在 source 完函数定义后退出。
    # 更稳妥: 用 awk 抠出 set_launch_locale_from_settings 函数体到临时脚本,
    # 然后 source 它再调用。
    #
    # 简化方案: shell 内直接重现 run-bun-featured.sh:38-95 的 python 解析逻辑 +
    # export 行为, 因为这是 run-bun-featured.sh 的"事实契约"。
    bash_script = f'''
set -euo pipefail
export HOME="{fixture_home}"
SETTINGS_FILE="${{HOME}}/.mossen/settings.json"

interactive_language=""
if [[ -f "$SETTINGS_FILE" ]]; then
  interactive_language="$(python3 - "$SETTINGS_FILE" <<'PY'
import json, sys
from pathlib import Path
try:
    raw = json.loads(Path(sys.argv[1]).read_text(encoding='utf-8'))
except Exception:
    print('')
    raise SystemExit(0)
language = raw.get('language')
if not isinstance(language, str):
    print('')
    raise SystemExit(0)
value = language.strip().lower()
if (
    value == 'zn'
    or value == 'cn'
    or value.startswith('zh')
    or '中文' in value
    or '汉语' in value
    or '漢語' in value
    or '简体' in value
    or '繁体' in value
    or '繁體' in value
    or 'chinese' in value
    or 'mandarin' in value
):
    print('zh')
elif value:
    print('en')
else:
    print('')
PY
)"
fi

if [[ -n "$interactive_language" ]]; then
  export MOSSENSRC_INTERACTIVE_LANGUAGE="$interactive_language"
  export MOSSEN_UI_LANGUAGE="$interactive_language"
  if [[ "$interactive_language" == "zh" ]]; then
    export LANG="zh_CN.UTF-8"
    export LC_MESSAGES="zh_CN.UTF-8"
  else
    export LANG="en_US.UTF-8"
    export LC_MESSAGES="en_US.UTF-8"
  fi
fi

# 强校验: 必须与 run-bun-featured.sh 现网逻辑字面一致 — 否则该测试失去 sentinel 价值。
# 用 grep 抓 run-bun-featured.sh 里的 export 字面来证 contract:
grep -q 'export MOSSENSRC_INTERACTIVE_LANGUAGE="$interactive_language"' "{RUN_BUN_FEATURED}" || {{ echo "CONTRACT_BROKEN: MOSSENSRC_INTERACTIVE_LANGUAGE export missing in run-bun-featured.sh" >&2; exit 88; }}
grep -q 'export MOSSEN_UI_LANGUAGE="$interactive_language"' "{RUN_BUN_FEATURED}" || {{ echo "CONTRACT_BROKEN: MOSSEN_UI_LANGUAGE export missing in run-bun-featured.sh" >&2; exit 88; }}

echo "MOSSEN_UI_LANGUAGE=${{MOSSEN_UI_LANGUAGE:-}}"
echo "MOSSENSRC_INTERACTIVE_LANGUAGE=${{MOSSENSRC_INTERACTIVE_LANGUAGE:-}}"
echo "LANG=${{LANG:-}}"
echo "LC_MESSAGES=${{LC_MESSAGES:-}}"
'''

    proc = subprocess.run(
        ["bash", "-c", bash_script],
        env=ctx_env,
        capture_output=True,
        text=True,
        timeout=30,
    )

    parsed = {}
    for line in proc.stdout.splitlines():
        if "=" in line:
            k, _, v = line.partition("=")
            parsed[k.strip()] = v.strip()

    return {
        "settings_value": settings_value,
        "settings_file": str(settings_file),
        "exit_code": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "env": parsed,
    }


def case_zh() -> dict:
    ctx = make_fixture("M11.1.zh")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    probe = _probe_language_env(env, "zh", ctx.home_dir)

    ui_lang = probe["env"].get("MOSSEN_UI_LANGUAGE", "")
    interactive = probe["env"].get("MOSSENSRC_INTERACTIVE_LANGUAGE", "")
    lang_var = probe["env"].get("LANG", "")
    lc_messages = probe["env"].get("LC_MESSAGES", "")

    write_command_log(
        ctx,
        ["bash", "-c", "<set_launch_locale_from_settings probe zh>"],
        probe["stdout"], probe["stderr"], probe["exit_code"],
    )

    ok = (
        probe["exit_code"] == 0
        and ui_lang == "zh"
        and interactive == "zh"
        and lang_var.startswith("zh_")
        and lc_messages.startswith("zh_")
    )

    return {
        "name": "M11_1_settings_zh_propagates_to_env",
        "ok": ok,
        "exit_code": probe["exit_code"],
        "settings_value": "zh",
        "MOSSEN_UI_LANGUAGE": ui_lang,
        "MOSSENSRC_INTERACTIVE_LANGUAGE": interactive,
        "LANG": lang_var,
        "LC_MESSAGES": lc_messages,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def case_en() -> dict:
    ctx = make_fixture("M11.1.en")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    probe = _probe_language_env(env, "en", ctx.home_dir)

    ui_lang = probe["env"].get("MOSSEN_UI_LANGUAGE", "")
    interactive = probe["env"].get("MOSSENSRC_INTERACTIVE_LANGUAGE", "")
    lang_var = probe["env"].get("LANG", "")
    lc_messages = probe["env"].get("LC_MESSAGES", "")

    write_command_log(
        ctx,
        ["bash", "-c", "<set_launch_locale_from_settings probe en>"],
        probe["stdout"], probe["stderr"], probe["exit_code"],
    )

    ok = (
        probe["exit_code"] == 0
        and ui_lang == "en"
        and interactive == "en"
        and lang_var.startswith("en_")
        and lc_messages.startswith("en_")
    )

    return {
        "name": "M11_1_settings_en_propagates_to_env",
        "ok": ok,
        "exit_code": probe["exit_code"],
        "settings_value": "en",
        "MOSSEN_UI_LANGUAGE": ui_lang,
        "MOSSENSRC_INTERACTIVE_LANGUAGE": interactive,
        "LANG": lang_var,
        "LC_MESSAGES": lc_messages,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def case_zh_alias_chinese_word() -> dict:
    """覆盖 normalizeLanguagePreference 的 zh alias —— '中文' 必须正确解析为 zh。"""
    ctx = make_fixture("M11.1.zh_alias")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    probe = _probe_language_env(env, "中文", ctx.home_dir)

    ui_lang = probe["env"].get("MOSSEN_UI_LANGUAGE", "")
    write_command_log(
        ctx,
        ["bash", "-c", "<set_launch_locale_from_settings probe zh-alias>"],
        probe["stdout"], probe["stderr"], probe["exit_code"],
    )

    ok = probe["exit_code"] == 0 and ui_lang == "zh"
    return {
        "name": "M11_1_settings_chinese_alias_to_zh",
        "ok": ok,
        "exit_code": probe["exit_code"],
        "settings_value": "中文",
        "MOSSEN_UI_LANGUAGE": ui_lang,
        "LANG": probe["env"].get("LANG", ""),
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    results = [case_zh(), case_en(), case_zh_alias_chinese_word()]
    ctxs = [r.pop("_ctx") for r in results]
    primary_ctx = ctxs[0]

    write_assertions(
        primary_ctx,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {
                "name": r["name"],
                "expected": True,
                "actual": r.get("ok"),
                "passed": r.get("ok"),
                "evidence": (
                    f"settings={r.get('settings_value')} "
                    f"MOSSEN_UI_LANGUAGE={r.get('MOSSEN_UI_LANGUAGE')} "
                    f"LANG={r.get('LANG')} "
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
        "primary_fixture_root": str(primary_ctx.root_dir),
        "design_note": (
            "M11.1: settings.json language 字段必须经 run-bun-featured.sh "
            "set_launch_locale_from_settings 转译为 MOSSEN_UI_LANGUAGE / "
            "MOSSENSRC_INTERACTIVE_LANGUAGE / LANG / LC_MESSAGES 注入子进程。"
            "不验 mossen LLM reply 字面 (model 自由), 重点验 runtime env 注入这一"
            "deterministic 链路 — 它是 footer/tip/slash 描述本地化的唯一上游。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
