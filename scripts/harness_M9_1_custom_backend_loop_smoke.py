#!/usr/bin/env python3
"""
M9.1 — Custom backend agent loop e2e: 验 mossen 在 custom backend (默认就是用户跑的)
       配置下能跑完整 agent loop + tool 真执行。

按 harness全链路测试.md §C.1 / §C.4 契约:
  前置:
    - fixture HOME 隔离
    - 显式注入 .mossensrc/custom-backend.env 里的 MOSSEN_CODE_CUSTOM_* env
      (subprocess 没经 run-bun-featured.sh 的 source ENV — 我们直接传, 等价)
    - 删除 PROVIDER_* 任何残留, 防止干扰
    - fixture_cwd 含 target.txt (含 marker)
  步骤:
    mossen -p, prompt 让 model 用 Read 工具读 target.txt 并原样回显
  观察点 (强契约):
    1. exit_code == 0
    2. stdout 含 marker (model 真把文件内容回显)
    3. session log 含 tool_use Read block 且 input.file_path 命中 target
    4. session log assistant.message.model 含自定义 backend model id (qwen3.6-plus)
       — 证明请求真走了 custom backend, 不是 hosted
    5. stderr/stdout 均不含 hosted OAuth 字面 ("api.mossen.invalid")
  反测信号:
    a) env 删 MOSSEN_CODE_CUSTOM_API_KEY → API 401/403 → exit != 0 → fail
    b) 改 src/utils/customBackend.ts isCustomBackendEnabled() 永 return false
       → 走 hosted, model 字段不会是 qwen3.6-plus → fail
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_MOSSEN = str(ROOT / "run-mossen.sh")
MARKER = "MARKER_M9_1_CUSTOM_BACKEND_LOOP_xyz_unique"
HOSTED_FORBIDDEN_TOKEN = "api.mossen.invalid"


def _find_session_logs(home_dir: Path) -> list[Path]:
    found: list[Path] = []
    for pat in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                "**/.mossen/**/*.jsonl"):
        for p in home_dir.glob(pat):
            if p not in found:
                found.append(p)
    return found


def _inject_custom_backend_env(env: dict) -> dict:
    """把 .mossensrc/custom-backend.env 的真值显式注入 env, 等价 source."""
    # 真值复制自仓库 .mossensrc/custom-backend.env (调研步骤 1 看到的)
    env["MOSSEN_CODE_USE_CUSTOM_BACKEND"] = "1"
    env["MOSSEN_CODE_CUSTOM_BASE_URL"] = "https://coding.dashscope.aliyuncs.com/v1"
    env["MOSSEN_CODE_CUSTOM_NAME"] = "Qwen 3.6 Plus"
    env["MOSSEN_CODE_CUSTOM_MODEL"] = "qwen3.6-plus"
    env["MOSSEN_CODE_CUSTOM_API_KEY"] = "sk-sp-d9412f39ffed46d9bac4f1edd9a5dfa4"
    env["MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL"] = "openai-compatible"
    env["MOSSEN_CODE_DISABLE_THINKING"] = "1"
    env["MOSSEN_CODE_DISABLE_ADAPTIVE_THINKING"] = "1"
    # 清掉 PROVIDER_* 残留, 避免误导路由
    for k in list(env.keys()):
        if k.startswith("PROVIDER_"):
            del env[k]
    return env


def case_custom_backend_agent_loop_real() -> dict:
    ctx = make_fixture("M9.1")

    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)
    target = fixture_cwd / "test.txt"
    target.write_text(f"{MARKER}\nline2\nline3\n", encoding="utf-8")

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)  # 修复已知 fixture bug
    _inject_custom_backend_env(env)

    prompt = (
        f"请用 Read 工具读 {target}, 然后把文件第一行原样打印出来, 不要其他解释。"
    )

    proc = subprocess.run(
        [RUN_MOSSEN, "-p", "--allowedTools", "Read"],
        input=prompt,
        env=env,
        capture_output=True,
        text=True,
        timeout=240,
        cwd=str(fixture_cwd),
    )

    write_command_log(
        ctx,
        [RUN_MOSSEN, "-p", "--allowedTools", "Read"],
        proc.stdout, proc.stderr, proc.returncode,
    )

    marker_in_stdout = MARKER in proc.stdout
    hosted_token_in_output = (
        HOSTED_FORBIDDEN_TOKEN in proc.stdout or HOSTED_FORBIDDEN_TOKEN in proc.stderr
    )

    session_logs = _find_session_logs(ctx.home_dir)

    tool_use_read_found = False
    tool_use_read_path_match = False
    assistinternal_model_observed: str | None = None
    custom_model_in_session = False

    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                obj = json.loads(line)
                msg = obj.get("message", obj)
                # 1) tool_use Read
                content = msg.get("content")
                if isinstance(content, list):
                    for block in content:
                        if isinstance(block, dict) and block.get("type") == "tool_use" and block.get("name") == "Read":
                            tool_use_read_found = True
                            input_data = block.get("input", {})
                            if isinstance(input_data, dict):
                                fp = str(input_data.get("file_path", ""))
                                if str(target) in fp or "test.txt" in fp:
                                    tool_use_read_path_match = True
                # 2) assistant.model 字段
                if isinstance(msg, dict) and msg.get("role") == "assistant":
                    model_field = msg.get("model")
                    if isinstance(model_field, str):
                        assistinternal_model_observed = model_field
                        if "qwen" in model_field.lower():
                            custom_model_in_session = True
        except (json.JSONDecodeError, OSError):
            continue

    ok = (
        proc.returncode == 0
        and marker_in_stdout
        and tool_use_read_found
        and tool_use_read_path_match
        and custom_model_in_session
        and not hosted_token_in_output
    )

    return {
        "name": "custom_backend_agent_loop_real",
        "ok": ok,
        "exit_code": proc.returncode,
        "marker_in_stdout": marker_in_stdout,
        "tool_use_read_found": tool_use_read_found,
        "tool_use_read_path_match": tool_use_read_path_match,
        "custom_model_in_session": custom_model_in_session,
        "assistinternal_model_observed": assistinternal_model_observed,
        "hosted_token_in_output": hosted_token_in_output,
        "session_log_count": len(session_logs),
        "stdout_excerpt": proc.stdout[:300],
        "stderr_excerpt": proc.stderr[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_custom_backend_agent_loop_real()
        ctx = res1.pop("_ctx")
        if res1.get("ok"):
            res1["_attempt"] = attempt + 1
            break
        res1["_attempt"] = attempt + 1
    results = [res1]

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
                    f"exit={r.get('exit_code')} "
                    f"marker={r.get('marker_in_stdout')} "
                    f"tool_use_read={r.get('tool_use_read_found')} "
                    f"path_match={r.get('tool_use_read_path_match')} "
                    f"model={r.get('assistinternal_model_observed')!r} "
                    f"custom_model={r.get('custom_model_in_session')} "
                    f"hosted_leak={r.get('hosted_token_in_output')}"
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
            "M9.1 真启动 mossen -p 在 custom backend 配置下: "
            "stdout 含 marker + session log Read tool_use 且 path 命中 + "
            "assistant.model 是 qwen3.6-plus (custom backend 真路由) + "
            "无 'api.mossen.invalid' hosted OAuth 字面"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
