#!/usr/bin/env python3
"""
M5.2 — 4 类 memory type frontmatter 真各自加载。

按 harness全链路测试.md §3.5 M5.2 契约 (W1 修正后):
  前置: 在 fixture memory dir 创建 4 个 .md, frontmatter type 分别为
        user / feedback / project / reference, body 各含 unique marker
  步骤: bun -e 调 scanMemoryFiles(dir, signal) 解析
  观察点:
    1. 返回 4 个 entry
    2. set([e.type for e in entries]) == {'user', 'feedback', 'project', 'reference'}
    3. 每个 entry filename 匹配 (type→filename 一一对应)
    4. 每个 entry description 来自 frontmatter (确认 frontmatter 真被解析)
  反测: 把 memdir/memoryTypes.ts:parseMemoryType 'feedback' 短路 → entry type 缺失 → fail
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

EXPECTED_TYPES = {"user", "feedback", "project", "reference"}

MARKERS = {
    "user": "MARKER_USER_TYPE_5_2",
    "feedback": "MARKER_FEEDBACK_TYPE_5_2",
    "project": "MARKER_PROJECT_TYPE_5_2",
    "reference": "MARKER_REFERENCE_TYPE_5_2",
}


def case_4_types_loaded() -> dict:
    ctx = make_fixture("M5.2")

    automem_dir = ctx.root_dir / "automem"
    automem_dir.mkdir(parents=True, exist_ok=True)

    for type_name, marker in MARKERS.items():
        fname = f"mem_{type_name}.md"
        content = (
            f"---\n"
            f"name: m52_{type_name}_memory\n"
            f"description: M5.2 {type_name} type fixture\n"
            f"type: {type_name}\n"
            f"---\n"
            f"\n{marker}\n"
        )
        (automem_dir / fname).write_text(content, encoding="utf-8")

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { scanMemoryFiles } from './memdir/memoryScan.ts';"
        f"const dir = {json.dumps(str(automem_dir))};"
        "const ctrl = new AbortController();"
        "const headers = await scanMemoryFiles(dir, ctrl.signal);"
        "process.stdout.write(JSON.stringify({"
        "  count: headers.length,"
        "  entries: headers.map(h => ({"
        "    filename: h.filename,"
        "    type: h.type,"
        "    description: h.description"
        "  }))"
        "}) + '\\n');"
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=60,
        env=env,
    )

    write_command_log(ctx, [RUN_BUN, "-e", "<scanMemoryFiles>"],
                      proc.stdout, proc.stderr, proc.returncode)

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
            "name": "4_types_real_loaded",
            "ok": False,
            "exit_code": proc.returncode,
            "stderr_excerpt": proc.stderr[:500],
            "stdout_excerpt": proc.stdout[:500],
            "_ctx": ctx,
        }

    count = parsed.get("count", 0)
    entries = parsed.get("entries", [])
    types_seen = {e.get("type") for e in entries if e.get("type")}
    type_to_filename = {e.get("type"): e.get("filename") for e in entries}
    expected_filename_per_type = {t: f"mem_{t}.md" for t in EXPECTED_TYPES}
    file_match_per_type = {
        t: type_to_filename.get(t) == expected_filename_per_type[t]
        for t in EXPECTED_TYPES
    }
    descriptions_per_type = {e.get("type"): e.get("description") for e in entries}
    description_match_per_type = {
        t: descriptions_per_type.get(t) == f"M5.2 {t} type fixture"
        for t in EXPECTED_TYPES
    }

    return {
        "name": "4_types_real_loaded",
        "ok": (
            proc.returncode == 0
            and count == 4
            and types_seen == EXPECTED_TYPES
            and all(file_match_per_type.values())
            and all(description_match_per_type.values())
        ),
        "exit_code": proc.returncode,
        "count": count,
        "types_seen": sorted(t for t in types_seen if t),
        "expected_types": sorted(EXPECTED_TYPES),
        "file_match_per_type": file_match_per_type,
        "description_match_per_type": description_match_per_type,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_4_types_loaded()
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
                    f"count={r.get('count')} "
                    f"types_seen={r.get('types_seen')} "
                    f"expected={r.get('expected_types')} "
                    f"file_match={r.get('file_match_per_type')} "
                    f"desc_match={r.get('description_match_per_type')}"
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
            "M5.2: 4 .md with frontmatter type=(user/feedback/project/reference), "
            "scanMemoryFiles must return 4 entries each with parsed type."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
