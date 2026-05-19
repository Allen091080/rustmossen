#!/usr/bin/env python3
"""
M5.6 — memory 文件变更 reload — 改 MOSSEN.md 后重启生效, 不被 cache 锁死。

按 harness全链路测试.md §C.1 契约:
  类似 M6.3 (skill reload) 但针对 project memory:
    1. fixture_cwd 写 MOSSEN.md v1, 含 marker MEMORY_RELOAD_V1_M5_6
    2. bun -e 进程 A: getMemoryFiles() 找 Project entry, 验 content 含 V1
    3. python 改 MOSSEN.md → v2, 含 MEMORY_RELOAD_V2_M5_6, 删 V1
    4. bun -e 进程 B (新独立 bun): 同样 getMemoryFiles, 验 v2 in B, v1 not in B

  强契约 (任一 fail 即 case fail):
    A: V1 in A, V2 not in A
    B: V2 in B, V1 not in B
    rc_a == 0, rc_b == 0

  反测信号: src/utils/mossenmd.ts 加 process-级缓存让 Project 内容不刷新
            (例如改 processMemoryFileCandidates 把 readFile 结果固定 V1)
            → V1 in B → fail

  CWD 处理: bun -e 通过 MOSSENSRC_LAUNCH_CWD env 把 mossen 内部 cwd 切到
  fixture_cwd. run-bun-featured.sh 自动 capture $PWD 为 LAUNCH_CWD,
  所以 subprocess.run(cwd=fixture_cwd) 即可 (env 也保险加).
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

V1_MARKER = "MEMORY_RELOAD_V1_M5_6"
V2_MARKER = "MEMORY_RELOAD_V2_M5_6"


def _write_memory(path: Path, marker: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        f"# Project memory M5.6\n\nVersion marker: {marker}\n",
        encoding="utf-8",
    )


def _bun_get_project_memory(env: dict, cwd: str) -> tuple[int, str, str, dict | None]:
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { setOriginalCwd, setProjectRoot } from './bootstrap/state.ts';"
        f"setOriginalCwd({json.dumps(cwd)});"
        f"setProjectRoot({json.dumps(cwd)});"
        "import { getMemoryFiles } from './utils/mossenmd.ts';"
        "const files = await getMemoryFiles();"
        "const projects = files.filter((f) => f.type === 'Project');"
        "process.stdout.write(JSON.stringify({"
        "  count: files.length,"
        "  projectCount: projects.length,"
        "  projectEntries: projects.map((p) => ({path: p.path, content: p.content}))"
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
    parsed = None
    for line in reversed((proc.stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                parsed = json.loads(line)
                break
            except json.JSONDecodeError:
                continue
    return proc.returncode, proc.stdout, proc.stderr, parsed


def case_memory_file_reload() -> dict:
    ctx = make_fixture("M5.6")

    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSENSRC_LAUNCH_CWD"] = str(fixture_cwd)

    project_md = fixture_cwd / "MOSSEN.md"
    _write_memory(project_md, V1_MARKER)

    # ---- Process A ----
    rc_a, out_a, err_a, parsed_a = _bun_get_project_memory(env, str(fixture_cwd))

    # ---- 改写 v2 ----
    _write_memory(project_md, V2_MARKER)

    # ---- Process B (新独立 bun) ----
    rc_b, out_b, err_b, parsed_b = _bun_get_project_memory(env, str(fixture_cwd))

    write_command_log(
        ctx,
        [RUN_BUN, "-e", "<getMemoryFiles A then B after rewrite>"],
        f"=== A ===\n{out_a}\n=== B ===\n{out_b}\n",
        f"=== A ===\n{err_a}\n=== B ===\n{err_b}\n",
        rc_b,
    )

    if not parsed_a:
        return {
            "name": "memory_reload_v1_to_v2",
            "ok": False,
            "stage": "A",
            "rc_a": rc_a,
            "stdout_a_excerpt": out_a[:500],
            "stderr_a_excerpt": err_a[:500],
            "_ctx": ctx,
        }
    if not parsed_b:
        return {
            "name": "memory_reload_v1_to_v2",
            "ok": False,
            "stage": "B",
            "rc_b": rc_b,
            "stdout_b_excerpt": out_b[:500],
            "stderr_b_excerpt": err_b[:500],
            "_ctx": ctx,
        }

    a_combined = json.dumps(parsed_a.get("projectEntries", []), ensure_ascii=False)
    b_combined = json.dumps(parsed_b.get("projectEntries", []), ensure_ascii=False)

    v1_in_A = V1_MARKER in a_combined
    v2_in_A = V2_MARKER in a_combined
    v1_in_B = V1_MARKER in b_combined
    v2_in_B = V2_MARKER in b_combined

    return {
        "name": "memory_reload_v1_to_v2",
        "ok": (
            rc_a == 0
            and rc_b == 0
            and v1_in_A
            and not v2_in_A
            and v2_in_B
            and not v1_in_B
        ),
        "rc_a": rc_a,
        "rc_b": rc_b,
        "v1_in_A": v1_in_A,
        "v2_in_A": v2_in_A,
        "v1_in_B": v1_in_B,
        "v2_in_B": v2_in_B,
        "a_project_count": parsed_a.get("projectCount"),
        "b_project_count": parsed_b.get("projectCount"),
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_memory_file_reload()
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
                    f"rc_a={r.get('rc_a')} rc_b={r.get('rc_b')} "
                    f"v1_in_A={r.get('v1_in_A')} v2_in_A={r.get('v2_in_A')} "
                    f"v1_in_B={r.get('v1_in_B')} v2_in_B={r.get('v2_in_B')} "
                    f"a_proj={r.get('a_project_count')} b_proj={r.get('b_project_count')}"
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
            "M5.6: 进程 A 加载 v1 → 改 MOSSEN.md → 独立进程 B 必须看 v2 "
            "(无 cross-process cache 锁 v1)。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
