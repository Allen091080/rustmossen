#!/usr/bin/env python3
"""
M5.1 — 进程 1 让 model 写 user-level memory 文件，进程 2 重启 loader 真读到。

按 harness全链路测试.md §3.5 M5.1 契约 (修正后):
  AutoMem (用户级持久 memory) 是 mossen "记住事实" 的真实存放位置 — 用 Write
  工具写到 autoMem dir 下的 .md 文件 (frontmatter type=user)。MOSSEN.md 是
  config 文件 (sensitive, 默认拒写) 不是用户记忆载体, 不在本测覆盖。

  用 MOSSEN_COWORK_MEMORY_PATH_OVERRIDE 把 autoMem dir 重定向到 fixture
  路径 (避免污染真实 ~/.mossen/projects/...)。

步骤:
  1. 启 mossen 进程 1 (单 shot -p), prompt 让 model 用 Write 工具写一个
     memory 文件 (user_pref_dark_mode.md) 含 marker 'MOSSEN_M5_1_USER_PREF_MARKER_xyz'
     到 autoMem dir (= override path)
  2. 物理验证: autoMem dir 下 .md 文件存在且含 marker
  3. 启第二个独立 bun 进程 (同 fixture env), 调 scanMemoryFiles(autoMemDir),
     验返回 entry filename 和 content path 真含 marker

观察点 (强契约, 任一失败 → fail):
  1. 进程 1 退出码 0
  2. autoMem dir 下至少 1 个 .md (排除 MEMORY.md) 含 marker
  3. 进程 2 scanMemoryFiles 返回 ≥1 entry
  4. 至少 1 个 entry, 直接读其 filePath, 内容含 marker (loader 真跨进程读到)

反测信号:
  - 改 src/memdir/memoryScan.ts:scanMemoryFiles 让 readdir 返回 [] → 进程 2
    count==0 → fail
  - 改 src/memdir/paths.ts:getAutoMemPathOverride 不读 env → loader 不指向
    fixture, 找不到 marker → fail
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
RUN_MOSSEN = str(ROOT / "run-mossen.sh")

MARKER = "MOSSEN_M5_1_USER_PREF_MARKER_xyz"


def _bun_scan_memory(automem_dir: Path, env: dict) -> dict:
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { scanMemoryFiles } from './memdir/memoryScan.ts';"
        f"const dir = {json.dumps(str(automem_dir))};"
        "const ctrl = new AbortController();"
        "const headers = await scanMemoryFiles(dir, ctrl.signal);"
        "const fs = await import('node:fs');"
        "const entries = headers.map(h => ({"
        "  filename: h.filename,"
        "  filePath: h.filePath,"
        "  type: h.type,"
        "  description: h.description,"
        "  bodyContent: fs.existsSync(h.filePath) ? fs.readFileSync(h.filePath, 'utf8') : null"
        "}));"
        "process.stdout.write(JSON.stringify({count: headers.length, entries}) + '\\n');"
    )
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=60,
        env=env,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"scan subprocess failed: rc={proc.returncode} stderr={proc.stderr[:500]!r}"
        )
    for line in reversed((proc.stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            return json.loads(line)
    raise RuntimeError(
        f"scan no json. stdout={proc.stdout[:300]!r} stderr={proc.stderr[:300]!r}"
    )


def case_write_then_restart_read() -> dict:
    ctx = make_fixture("M5.1")

    automem_dir = ctx.root_dir / "automem"
    automem_dir.mkdir(parents=True, exist_ok=True)

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_COWORK_MEMORY_PATH_OVERRIDE"] = str(automem_dir)

    target_file = automem_dir / "user_pref_dark_mode.md"

    prompt = (
        f"请用 Write 工具创建文件 {target_file}, 内容写: "
        f"---\\nname: user_pref\\ntype: user\\ndescription: M5.1 fixture\\n---\\n"
        f"{MARKER}\\n. 完成后回复 OK."
    )

    p1 = subprocess.run(
        [
            RUN_MOSSEN,
            "-p", prompt,
            "--allowedTools", "Write",
            "--add-dir", str(automem_dir),
        ],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=240,
        env=env,
    )

    write_command_log(
        ctx,
        [RUN_MOSSEN, "-p", "<write marker>", "--allowedTools", "Write",
         "--add-dir", str(automem_dir)],
        p1.stdout, p1.stderr, p1.returncode,
    )

    md_files = [p for p in automem_dir.glob("*.md") if p.name != "MEMORY.md"]
    file_with_marker = None
    for f in md_files:
        if MARKER in f.read_text(encoding="utf-8"):
            file_with_marker = f
            break

    scan_result: dict | None = None
    scan_error: str | None = None
    entry_with_marker = None
    try:
        scan_result = _bun_scan_memory(automem_dir, env=env)
    except Exception as e:
        scan_error = str(e)

    if scan_result is not None:
        for entry in scan_result.get("entries", []):
            if MARKER in (entry.get("bodyContent") or ""):
                entry_with_marker = entry
                break

    ok = (
        p1.returncode == 0
        and file_with_marker is not None
        and scan_result is not None
        and entry_with_marker is not None
    )

    return {
        "name": "M5_1_write_then_restart_read",
        "ok": ok,
        "process1_exit": p1.returncode,
        "automem_dir": str(automem_dir),
        "md_files_found": [str(p) for p in md_files],
        "file_with_marker": str(file_with_marker) if file_with_marker else None,
        "scan_error": scan_error,
        "scan_count": (scan_result or {}).get("count"),
        "entry_with_marker_path": entry_with_marker.get("filePath") if entry_with_marker else None,
        "stdout_excerpt": (p1.stdout or "")[:400],
        "stderr_excerpt": (p1.stderr or "")[:400],
        "_ctx": ctx,
    }


def main() -> int:
    res = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res = case_write_then_restart_read()
        ctx = res.pop("_ctx")
        if res.get("ok"):
            res["_attempt"] = attempt + 1
            break
        res["_attempt"] = attempt + 1
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
                    f"p1_exit={r.get('process1_exit')} "
                    f"file_with_marker={r.get('file_with_marker')} "
                    f"scan_count={r.get('scan_count')} "
                    f"entry_with_marker_path={r.get('entry_with_marker_path')} "
                    f"scan_error={r.get('scan_error')}"
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
            "M5.1: process1 mossen Write marker to autoMem (override path), "
            "process2 fresh bun scanMemoryFiles must return entry whose file content has marker."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
