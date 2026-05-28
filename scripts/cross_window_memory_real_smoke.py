#!/usr/bin/env python3
"""
cross_window_memory_real_smoke.py — P1-4 真完成度补强。

之前 cross_window_memory_smoke.py 只验路径计算一致性（不变量层），
没真双进程跑 write → read 闭环。本 smoke 真起 2 个独立 bun process
模拟"窗口 A 写 / 窗口 B 读"，验证持久化跨进程可见。

场景：
  A: bun process 1 调用 mossenmd loader → 写一个 unique marker 到 AutoMem MEMORY.md
  B: bun process 2 启动 → 调用 getMemoryFiles → 验证能读到 A 写入的 marker

为什么不用 /memory add CLI:
  - mossen --print 模式不支持 /memory add 交互
  - 直接用 mossenmd loader 路径更确定地测中 cross-window 这条链路
  - 真用户用 /memory add 也是同一份底层路径

清理：
  smoke 写入有特殊 marker（_TEST_<timestamp>_），跑完自动从文件中删除。
  失败时不删（方便排查）。
"""

from __future__ import annotations

import json
import os
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN_BUN = str(ROOT / "run-bun-featured.sh")


def _bun(snippet: str, env: dict[str, str] | None = None) -> dict:
    full_env = os.environ.copy()
    if env:
        full_env.update(env)
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=60,
        env=full_env,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"bun rc={proc.returncode} stderr={proc.stderr[:500]!r} stdout={proc.stdout[:300]!r}"
        )
    out = (proc.stdout or "").strip()
    for line in reversed(out.splitlines()):
        line = line.strip()
        if line.startswith("{") or line.startswith("["):
            return json.loads(line)
    raise RuntimeError(
        f"no json — stdout={out[:300]!r} stderr={proc.stderr[:500]!r}"
    )


def main() -> int:
    marker = f"_CROSS_WINDOW_SMOKE_TEST_MARKER_{int(time.time())}_"

    # === Process A: write marker to AutoMem MEMORY.md ===
    write_snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { getAutoMemEntrypoint } from './memdir/paths.ts';"
        "import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';"
        "import { dirname } from 'node:path';"
        "const path = getAutoMemEntrypoint();"
        "mkdirSync(dirname(path), { recursive: true });"
        f"const marker = '{marker}';"
        "const old = existsSync(path) ? readFileSync(path, 'utf8') : '';"
        "writeFileSync(path, old + '\\n' + marker + '\\n');"
        "console.log(JSON.stringify({wrote: true, path, marker, total_size: (old + marker).length}));"
    )

    try:
        write_result = _bun(write_snippet)
    except Exception as e:
        print(json.dumps({"step": "write", "error": str(e)}, indent=2))
        return 1

    write_path = write_result["path"]

    # === Process B (独立 bun, 真模拟 mossen 启动加载 memory) ===
    # 不再用 raw readFileSync — 改用 mossen 自己的 getMemoryFiles() loader
    # 这是 mossen 启动时真实加载项目记忆的代码路径
    read_snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { getAutoMemEntrypoint } from './memdir/paths.ts';"
        "import { getMemoryFiles } from './utils/mossenmd.ts';"
        f"const expectedMarker = '{marker}';"
        "const path = getAutoMemEntrypoint();"
        "const files = await getMemoryFiles();"
        "const automemEntry = files.find((f: any) => f.path === path);"
        "const found_in_loader = files.some((f: any) => (f.content ?? '').includes(expectedMarker));"
        "console.log(JSON.stringify({"
        "  path,"
        "  loader_returned_count: files.length,"
        "  loader_includes_automem: automemEntry !== undefined,"
        "  found_in_loader,"
        "  found_in_automem_entry: automemEntry ? (automemEntry.content ?? '').includes(expectedMarker) : false,"
        "  loaded_types: [...new Set(files.map((f: any) => f.type))],"
        "}));"
    )

    try:
        read_result = _bun(read_snippet)
    except Exception as e:
        print(json.dumps({"step": "read", "error": str(e)}, indent=2))
        return 1

    # Cleanup: remove the marker line
    cleanup_snippet = (
        "import { existsSync, readFileSync, writeFileSync } from 'node:fs';"
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "import { getAutoMemEntrypoint } from './memdir/paths.ts';"
        "const path = getAutoMemEntrypoint();"
        "if (existsSync(path)) {"
        "  const old = readFileSync(path, 'utf8');"
        f"  const cleaned = old.split('\\n').filter(l => !l.includes('{marker}')).join('\\n');"
        "  writeFileSync(path, cleaned);"
        "  console.log(JSON.stringify({cleaned: true}));"
        "} else { console.log(JSON.stringify({cleaned: false, reason: 'no file'})); }"
    )

    cleanup_ok = False
    try:
        _bun(cleanup_snippet)
        cleanup_ok = True
    except Exception:
        pass

    # === Verification ===
    same_path = write_result["path"] == read_result["path"]
    found_via_mossen_loader = read_result.get("found_in_loader") is True

    summary = {
        "marker": marker,
        "process_a_wrote": write_result,
        "process_b_read_via_mossen_loader": read_result,
        "same_path": same_path,
        "marker_visible_via_mossen_getMemoryFiles": found_via_mossen_loader,
        "cleanup_ok": cleanup_ok,
        "verdict": (
            "OK_MOSSEN_LOADER_SEES_CROSS_WINDOW_WRITE"
            if same_path and found_via_mossen_loader
            else "FAILED"
        ),
        "test_design": (
            "Process A writes via mossen's getAutoMemEntrypoint() path. "
            "Process B (independent bun) calls mossen's getMemoryFiles() loader "
            "and verifies marker appears in loaded entries — this is the same "
            "code path mossen runs at startup to load project memory."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if same_path and found_via_mossen_loader else 1


if __name__ == "__main__":
    raise SystemExit(main())
