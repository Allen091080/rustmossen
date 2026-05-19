#!/usr/bin/env python3
"""
M6.3 — skill reload: 改 SKILL.md 后, 新进程加载到 v2 内容 (cache 不锁死)。

按 harness全链路测试.md §3.6 M6.3 契约:
  前置: fixture mossen_config_home/skills/m63_reload_skill/SKILL.md v1
        body 含 marker 'SKILL_RELOAD_V1_M6_3'
  步骤:
    1. 进程 A (bun -e): getSkillDirCommands(cwd) 获取该 skill, 触发其 prompt,
       记录 prompt body —— 验含 V1 marker
    2. python 改写 SKILL.md → v2 (含 'SKILL_RELOAD_V2_M6_3', 删 V1)
    3. 进程 B (新独立 bun -e): 同样 getSkillDirCommands → 取 prompt body
       —— 验含 V2 marker, 不含 V1 marker
  观察点:
    A 阶段: v1_marker_in_A == True, v2_marker_in_A == False
    B 阶段: v1_marker_in_B == False, v2_marker_in_B == True
  反测: 改 src/skills/loadSkillsDir.ts 让 loadSkillsFromSkillsDir 永不刷新缓存
        (e.g. 给整个 cache 加 process-级 memo 永不清) → 进程 B 仍看 V1 → fail
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

SKILL_NAME = "m63_reload_skill"
V1_MARKER = "SKILL_RELOAD_V1_M6_3"
V2_MARKER = "SKILL_RELOAD_V2_M6_3"


def _write_skill(skill_md_path: Path, marker: str, version_label: str) -> None:
    skill_md_path.parent.mkdir(parents=True, exist_ok=True)
    content = (
        f"---\n"
        f"name: {SKILL_NAME}\n"
        f"description: M6.3 reload skill ({version_label}) marker {marker}\n"
        f"user-invocable: true\n"
        f"---\n"
        f"\n{marker}\n"
    )
    skill_md_path.write_text(content, encoding="utf-8")


def _bun_load_skill(env: dict, cwd: str) -> tuple[int, str, str]:
    """跑独立 bun 进程, 调 getSkillDirCommands → 取 m63 skill 的 prompt body。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const { getSkillDirCommands } = await import('./skills/loadSkillsDir.ts');"
        "const skills = await getSkillDirCommands(process.cwd());"
        f"const target = skills.find(s => s.name === {json.dumps(SKILL_NAME)});"
        "if (!target) {"
        "  process.stdout.write(JSON.stringify({ found: false, names: skills.map(s => s.name) }) + '\\n');"
        "  process.exit(0);"
        "}"
        "const blocks = await target.getPromptForCommand('', { abortController: new AbortController(), readFileTimeoutMs: 30000, options: { commands: [], tools: [] }, messageId: 'm63', agentId: 'm63', setToolJSX: () => {}, getQueuedCommands: () => [], removeQueuedCommands: () => {}, getMcpClients: () => [] });"
        "const text = blocks.map(b => b.type === 'text' ? b.text : '').join('\\n');"
        "process.stdout.write(JSON.stringify({"
        "  found: true,"
        "  description: target.description,"
        "  prompt_body: text"
        "}) + '\\n');"
    )
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=cwd,
        text=True,
        capture_output=True,
        timeout=60,
        env=env,
    )
    return proc.returncode, proc.stdout, proc.stderr


def _parse_last_json(stdout: str) -> dict | None:
    for line in reversed((stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return None


def case_skill_reload() -> dict:
    ctx = make_fixture("M6.3")

    skill_md = ctx.mossen_config_home / "skills" / SKILL_NAME / "SKILL.md"
    _write_skill(skill_md, V1_MARKER, "v1")

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    # ---- 进程 A: 取 v1 -------------------------------------------------
    rc_a, out_a, err_a = _bun_load_skill(env, str(ROOT))
    parsed_a = _parse_last_json(out_a)

    # ---- 改 SKILL.md → v2 ---------------------------------------------
    _write_skill(skill_md, V2_MARKER, "v2")

    # ---- 进程 B: 新独立 bun, 取 v2 -----------------------------------
    rc_b, out_b, err_b = _bun_load_skill(env, str(ROOT))
    parsed_b = _parse_last_json(out_b)

    write_command_log(
        ctx,
        [RUN_BUN, "-e", "<getSkillDirCommands x2 (A then B after rewrite)>"],
        f"=== A ===\n{out_a}\n=== B ===\n{out_b}",
        f"=== A ===\n{err_a}\n=== B ===\n{err_b}",
        rc_b,
    )

    if not parsed_a or not parsed_a.get("found"):
        return {
            "name": "skill_reload_v1_to_v2",
            "ok": False,
            "stage": "A",
            "exit_code_A": rc_a,
            "stderr_A_excerpt": err_a[:500],
            "stdout_A_excerpt": out_a[:500],
            "_ctx": ctx,
        }
    if not parsed_b or not parsed_b.get("found"):
        return {
            "name": "skill_reload_v1_to_v2",
            "ok": False,
            "stage": "B",
            "exit_code_B": rc_b,
            "stderr_B_excerpt": err_b[:500],
            "stdout_B_excerpt": out_b[:500],
            "_ctx": ctx,
        }

    body_a = (parsed_a.get("prompt_body") or "") + " " + (parsed_a.get("description") or "")
    body_b = (parsed_b.get("prompt_body") or "") + " " + (parsed_b.get("description") or "")

    v1_in_A = V1_MARKER in body_a
    v2_in_A = V2_MARKER in body_a
    v1_in_B = V1_MARKER in body_b
    v2_in_B = V2_MARKER in body_b

    return {
        "name": "skill_reload_v1_to_v2",
        "ok": (
            rc_a == 0
            and rc_b == 0
            and v1_in_A
            and not v2_in_A
            and v2_in_B
            and not v1_in_B
        ),
        "exit_code_A": rc_a,
        "exit_code_B": rc_b,
        "v1_in_A": v1_in_A,
        "v2_in_A": v2_in_A,
        "v1_in_B": v1_in_B,
        "v2_in_B": v2_in_B,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_skill_reload()
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
                    f"exit_A={r.get('exit_code_A')} exit_B={r.get('exit_code_B')} "
                    f"v1_in_A={r.get('v1_in_A')} v2_in_A={r.get('v2_in_A')} "
                    f"v1_in_B={r.get('v1_in_B')} v2_in_B={r.get('v2_in_B')}"
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
            "M6.3: 进程 A 加载 v1 → 改 SKILL.md → 独立进程 B 必须看到 v2, "
            "不能仍看 v1 (排除 cross-process 永久缓存)。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
