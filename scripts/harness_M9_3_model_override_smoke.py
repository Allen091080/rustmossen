#!/usr/bin/env python3
"""
M9.3 — CLI --model 覆盖在 custom backend 下生效, session log 真出现指定 model id。

按 harness全链路测试.md §C.1 / §C.4 契约:
  custom backend 默认 model 由 MOSSEN_CODE_CUSTOM_MODEL 指定 (env 默认 qwen3.6-plus)。
  CLI --model <id> 应该能覆盖这个默认值, 让本次会话的 assistant 消息 model
  字段反映指定值, statusline / session log 一致。

  前置:
    - fixture HOME 隔离 + custom backend env (有 valid API key)
    - 故意把默认 MOSSEN_CODE_CUSTOM_MODEL 设为 sentinel "qwen3.6-plus"
  步骤:
    mossen -p --model qwen3.6-plus simple prompt
    (用 valid 的 dashscope model id, 否则 API 直接 400)
  观察点 (强契约):
    1. exit_code == 0
    2. session log 至少 1 条 assistant message 的 message.model 字段命中
       requested model id 或其 server-canonical 形式
       (custom backend 把 model 透传到 OpenAI-compatible body)
    3. 至少 1 条 user message 真发送过 (proves agent loop 跑了)
  反测信号:
    a) 改 src/main.tsx setMainLoopModelOverride() 改成 no-op
       → assistant.model 仍是默认 → 不命中 requested → fail
    b) 改 src/services/api/openaiCompatibleClient.ts 把 model 写死
       → assistant.model 全是写死值 → fail

调研:
  src/main.tsx:1035 .option('--model <model>')
  src/main.tsx:2152 setMainLoopModelOverride(effectiveModel)
  src/utils/customBackend.ts:179 getCustomBackendModel() = MOSSEN_CODE_CUSTOM_MODEL
  jsonl assistant.message.model 真值是 backend 返回的 (e.g. 'qwen3.6-plus')
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

# 用 dashscope 真有效 model id (跟 .mossensrc/custom-backend.env 默认值一致)
# CLI --model 显式传, 验整链路 plumbing
REQUESTED_MODEL = "qwen3.6-plus"


def _find_session_logs(home_dir: Path) -> list[Path]:
    found: list[Path] = []
    for pat in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                "**/.mossen/**/*.jsonl"):
        for p in home_dir.glob(pat):
            if p not in found:
                found.append(p)
    return found


def _inject_custom_backend_env(env: dict) -> dict:
    env["MOSSEN_CODE_USE_CUSTOM_BACKEND"] = "1"
    env["MOSSEN_CODE_CUSTOM_BASE_URL"] = "https://coding.dashscope.aliyuncs.com/v1"
    env["MOSSEN_CODE_CUSTOM_NAME"] = "Qwen 3.6 Plus"
    # sentinel: env 默认设个 SENTINEL model 名, 让 CLI --model 真承担覆盖任务
    # 如果 --model 真生效 → assistant.model 显示 REQUESTED (qwen3.6-plus)
    # 如果 --model 被忽略 → assistant.model 显示 SENTINEL (qwen-sentinel-default)
    env["MOSSEN_CODE_CUSTOM_MODEL"] = "qwen-sentinel-default-M9_3"
    env["MOSSEN_CODE_CUSTOM_API_KEY"] = "sk-sp-d9412f39ffed46d9bac4f1edd9a5dfa4"
    env["MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL"] = "openai-compatible"
    env["MOSSEN_CODE_DISABLE_THINKING"] = "1"
    env["MOSSEN_CODE_DISABLE_ADAPTIVE_THINKING"] = "1"
    for k in list(env.keys()):
        if k.startswith("ANTHROPIC_"):
            del env[k]
    return env


def case_model_override_via_cli() -> dict:
    ctx = make_fixture("M9.3")

    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    _inject_custom_backend_env(env)

    prompt = "回复一个字: ok"

    proc = subprocess.run(
        [RUN_MOSSEN, "-p", "--model", REQUESTED_MODEL],
        input=prompt,
        env=env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(fixture_cwd),
    )

    write_command_log(
        ctx,
        [RUN_MOSSEN, "-p", "--model", REQUESTED_MODEL],
        proc.stdout, proc.stderr, proc.returncode,
    )

    session_logs = _find_session_logs(ctx.home_dir)

    user_msg_observed = False
    assistant_msg_count = 0
    assistant_models: list[str] = []
    requested_model_in_session = False

    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                obj = json.loads(line)
                msg = obj.get("message", obj)
                if not isinstance(msg, dict):
                    continue
                role = msg.get("role")
                if role == "user":
                    user_msg_observed = True
                elif role == "assistant":
                    assistant_msg_count += 1
                    model_field = msg.get("model")
                    if isinstance(model_field, str):
                        assistant_models.append(model_field)
                        # 命中标准: requested id 完全包含或被包含 (兼容 backend 返回 'qwen3.6-plus' 之类)
                        m_lower = model_field.lower()
                        if (
                            REQUESTED_MODEL.lower() in m_lower
                            or m_lower in REQUESTED_MODEL.lower()
                        ):
                            requested_model_in_session = True
        except (json.JSONDecodeError, OSError):
            continue

    ok = (
        proc.returncode == 0
        and user_msg_observed
        and assistant_msg_count > 0
        and requested_model_in_session
    )

    return {
        "name": "model_override_via_cli",
        "ok": ok,
        "exit_code": proc.returncode,
        "user_msg_observed": user_msg_observed,
        "assistant_msg_count": assistant_msg_count,
        "assistant_models": list(dict.fromkeys(assistant_models))[:5],  # 去重前 5
        "requested_model": REQUESTED_MODEL,
        "requested_model_in_session": requested_model_in_session,
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
        res1 = case_model_override_via_cli()
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
                    f"user_msg={r.get('user_msg_observed')} "
                    f"asst_count={r.get('assistant_msg_count')} "
                    f"requested={r.get('requested_model')!r} "
                    f"models_seen={r.get('assistant_models')} "
                    f"hit={r.get('requested_model_in_session')}"
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
            f"M9.3 真启动 mossen -p --model {REQUESTED_MODEL} 在 custom backend 下: "
            "session log 至少 1 条 assistant.message.model 真命中 requested model id, "
            "证明 CLI --model 覆盖路径在 custom backend 下完整 plumbed (不是被吞掉)"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
