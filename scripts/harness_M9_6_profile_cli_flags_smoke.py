#!/usr/bin/env python3
"""
M9.6 — multi-profile CLI flags on the current Rust entrypoint (S1-09c P0).

按 Allen D-S09-2=Z + Stage1 §11.6 契约:
  对外 profile flags 必须在当前 Rust CLI fast-path 处理 (不进 mossen 主流程):
    --list-model-profiles
    --get-model-profile [<name>]
    --set-model-profile <name>
    --add-model-profile <name> --provider X --baseURL Y --model M --apiKey K [--name N]
    --update-model-profile <name> [partial fields]
    --set-model-profile-key <name> <key>
    --delete-model-profile <name>

  关键 case (链式 9 步):
    1. --list (空) → exit 0, count=0
    2. --add sample → exit 0, profile in settings.json
    3. --add fast → exit 0, count=2
    4. --list → 2 profiles, apiKey 真脱敏 (前 6 ... 后 4)
    5. --set sample → activeProfile=sample
    6. --get (无参 = active) → 返回 sample 脱敏
    7. --update sample --model new-model → model 真换
    8. --set-model-profile-key sample new-key → apiKey 真换
    9. --delete sample → 1 剩, activeProfile cleared

  反测信号:
    a) 删 Rust CLI fast-path 注册 → flag 进主流程, exit 非 0 (mossen 不识别)
    b) profile_cli.rs 不调 desensitize_profile → list/get 输出含 apiKey 字面
    c) --add 不查 getProfileByName(name) → 重复 add 不报错 → fail
    d) --delete cascade clear active 失败 → activeProfileCleared=false → fail
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_MOSSEN = str(ROOT / "scripts" / "start-mossen.sh")

SAMPLE_KEY_INIT = "sk-test-sample-AAAAAAAAAAAAAAAA"
SAMPLE_KEY_NEW = "sk-test-sample-NEWKEY-XXXXXXXXX"
MINIMAX_KEY = "sk-test-fast-BBBBBBBBBBBBBBB"


def _run_mossen(env: dict, args: list[str]) -> tuple[int, str, str]:
    proc = subprocess.run(
        [RUN_MOSSEN, *args],
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=120,
    )
    return proc.returncode, proc.stdout, proc.stderr


def _parse_last_json(stdout: str) -> dict | None:
    """fast-path 输出整段 JSON. 直接 json.loads stdout."""
    try:
        return json.loads(stdout)
    except json.JSONDecodeError:
        # 可能有多行 (json 后跟 newline), 尝试 strip
        for line in reversed(stdout.splitlines()):
            line = line.strip()
            if line.startswith("{"):
                try:
                    return json.loads(line)
                except json.JSONDecodeError:
                    continue
            # JSON dump 跨多行, try parse from start
        try:
            return json.loads(stdout.strip())
        except json.JSONDecodeError:
            return None


def case_full_lifecycle() -> dict:
    """9-步链式: list/add/add/list/set-active/get/update/set-key/delete"""
    ctx = make_fixture("M9.6.full_lifecycle")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    settings_dir = ctx.mossen_config_home
    settings_dir.mkdir(parents=True, exist_ok=True)
    (settings_dir / "settings.json").write_text("{}", encoding="utf-8")

    trace = []
    overall_ok = True

    def step(name: str, args: list[str], expect_exit: int = 0) -> dict:
        nonlocal overall_ok
        rc, stdout, stderr = _run_mossen(env, args)
        parsed = _parse_last_json(stdout)
        ok = rc == expect_exit and (parsed is not None or expect_exit != 0)
        if not ok:
            overall_ok = False
        result = {
            "step": name,
            "args": args,
            "exit_code": rc,
            "expect_exit": expect_exit,
            "parsed": parsed,
            "stderr_excerpt": stderr[:300],
            "stdout_excerpt": stdout[:300],
            "ok": ok,
        }
        trace.append(result)
        return result

    # Step 1: empty list
    s1 = step("list_empty", ["--list-model-profiles"])
    if s1["parsed"]:
        if not (s1["parsed"].get("count") == 0 and s1["parsed"].get("activeProfile") is None):
            overall_ok = False

    # Step 2: add sample
    s2 = step("add_sample", [
        "--add-model-profile", "sample",
        "--provider", "openai-compatible",
        "--baseURL", "https://example.com/sample/v1",
        "--model", "sample-test-1",
        "--apiKey", SAMPLE_KEY_INIT,
        "--name", "Test Qwen",
    ])
    if s2["parsed"]:
        if not (
            s2["parsed"].get("ok") is True
            and s2["parsed"].get("action") == "add"
            and s2["parsed"].get("name") == "sample"
        ):
            overall_ok = False

    # Step 3: add fast
    s3 = step("add_fast", [
        "--add-model-profile", "fast",
        "--provider", "openai-compatible",
        "--baseURL", "https://example.com/fast/v1",
        "--model", "fast-test",
        "--apiKey", MINIMAX_KEY,
    ])

    # Step 4: list 2
    s4 = step("list_two", ["--list-model-profiles"])
    if s4["parsed"]:
        profiles = s4["parsed"].get("profiles") or {}
        sample_d = profiles.get("sample") or {}
        fast_d = profiles.get("fast") or {}
        if not (
            s4["parsed"].get("count") == 2
            and "sample" in profiles
            and "fast" in profiles
            and sample_d.get("apiKey", "").startswith(SAMPLE_KEY_INIT[:6])
            and sample_d.get("apiKey") != SAMPLE_KEY_INIT  # 强契约: 脱敏后不等于原 key
            and fast_d.get("apiKey") != MINIMAX_KEY
            and "..." in sample_d.get("apiKey", "")
        ):
            overall_ok = False

    # Step 5: set active = sample
    s5 = step("set_active_sample", ["--set-model-profile", "sample"])
    if s5["parsed"]:
        if s5["parsed"].get("activeProfile") != "sample":
            overall_ok = False

    # Step 6: get (no name = active)
    s6 = step("get_active", ["--get-model-profile"])
    if s6["parsed"]:
        prof = s6["parsed"].get("profile") or {}
        if not (
            s6["parsed"].get("name") == "sample"
            and prof.get("model") == "sample-test-1"
            and prof.get("apiKey", "").startswith(SAMPLE_KEY_INIT[:6])
        ):
            overall_ok = False

    # Step 7: update sample --model new-model
    s7 = step("update_sample_model", [
        "--update-model-profile", "sample",
        "--model", "sample-test-2",
    ])
    if s7["parsed"]:
        if not (
            s7["parsed"].get("action") == "update"
            and s7["parsed"].get("name") == "sample"
        ):
            overall_ok = False

    s7_get = step("get_sample_after_update", ["--get-model-profile", "sample"])
    if s7_get["parsed"]:
        prof = s7_get["parsed"].get("profile") or {}
        if not (
            s7_get["parsed"].get("name") == "sample"
            and prof.get("model") == "sample-test-2"
            and prof.get("baseURL") == "https://example.com/sample/v1"  # 未指定的字段保留
            and SAMPLE_KEY_INIT not in json.dumps(s7_get["parsed"])
        ):
            overall_ok = False

    # Step 8: set-key sample new-key
    s8 = step("set_key_sample", [
        "--set-model-profile-key", "sample", SAMPLE_KEY_NEW,
    ])
    if s8["parsed"]:
        if not (
            s8["parsed"].get("action") == "set-key"
            and s8["parsed"].get("name") == "sample"
        ):
            overall_ok = False

    s8_get = step("get_sample_after_set_key", ["--get-model-profile", "sample"])
    if s8_get["parsed"]:
        prof = s8_get["parsed"].get("profile") or {}
        if not (
            prof.get("apiKey", "").startswith(SAMPLE_KEY_NEW[:6])
            and "..." in prof.get("apiKey", "")
            and prof.get("apiKey") != SAMPLE_KEY_NEW
            and prof.get("model") == "sample-test-2"  # 其他字段保留
            and SAMPLE_KEY_NEW not in json.dumps(s8_get["parsed"])
        ):
            overall_ok = False

    # Step 9: delete sample
    s9 = step("delete_sample", ["--delete-model-profile", "sample"])
    if s9["parsed"]:
        if not (
            s9["parsed"].get("deleted") is True
            and s9["parsed"].get("activeProfileCleared") is True
            and s9["parsed"].get("remainingProfiles") == ["fast"]
        ):
            overall_ok = False

    # Final settings.json 持久化校验
    persisted = json.loads((settings_dir / "settings.json").read_text(encoding="utf-8"))
    persisted_profiles = persisted.get("mossen.profiles") or {}
    persisted_active = persisted.get("mossen.activeProfile")
    if not (
        sorted(persisted_profiles.keys()) == ["fast"]
        and persisted_profiles["fast"]["apiKey"] == MINIMAX_KEY  # 真值持久化, 没在文件被脱敏
        and persisted_active in (None, "")
    ):
        overall_ok = False

    write_command_log(
        ctx,
        [RUN_MOSSEN, "<full lifecycle 9 steps>"],
        json.dumps([t["stdout_excerpt"] for t in trace]),
        json.dumps([t["stderr_excerpt"] for t in trace]),
        sum(t["exit_code"] for t in trace),
    )

    return {
        "name": "M9_6_profile_cli_full_lifecycle_9_steps",
        "ok": overall_ok,
        "step_count": len(trace),
        "step_summary": [{"step": t["step"], "exit": t["exit_code"], "ok": t["ok"]} for t in trace],
        "persisted_profiles": sorted(persisted_profiles.keys()),
        "persisted_active": persisted_active,
        "persisted_fast_key_intact": persisted_profiles.get("fast", {}).get("apiKey") == MINIMAX_KEY,
        "_ctx": ctx,
    }


def case_validation_failures() -> dict:
    """校验失败 case: 缺字段 / 不存在 profile / 重复 add → 必须 exit 1, 不污染 settings"""
    ctx = make_fixture("M9.6.validation")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    settings_dir = ctx.mossen_config_home
    settings_dir.mkdir(parents=True, exist_ok=True)
    (settings_dir / "settings.json").write_text("{}", encoding="utf-8")

    # 1. add 缺 --apiKey → exit 1
    rc1, _, stderr1 = _run_mossen(env, [
        "--add-model-profile", "x",
        "--provider", "openai-compatible",
        "--baseURL", "https://example.com",
        "--model", "m",
    ])

    # 2. set 不存在 → exit 1
    rc2, _, stderr2 = _run_mossen(env, ["--set-model-profile", "ghost"])

    # 3. update 不存在 → exit 1
    rc3, _, stderr3 = _run_mossen(env, ["--update-model-profile", "ghost", "--model", "x"])

    # 4. delete 不存在 → exit 1 (deleted=false)
    rc4, stdout4, _ = _run_mossen(env, ["--delete-model-profile", "ghost"])
    parsed4 = _parse_last_json(stdout4) or {}

    # 5. 真 add 后, 重复 add → exit 1
    rc5a, _, _ = _run_mossen(env, [
        "--add-model-profile", "y",
        "--provider", "openai-compatible",
        "--baseURL", "https://x.com", "--model", "m", "--apiKey", "k",
    ])
    rc5b, _, stderr5b = _run_mossen(env, [
        "--add-model-profile", "y",
        "--provider", "openai-compatible",
        "--baseURL", "https://x.com", "--model", "m", "--apiKey", "k",
    ])

    # 验最终 settings.json 只有 'y' 一个 profile (failed cmds 没污染)
    persisted = json.loads((settings_dir / "settings.json").read_text(encoding="utf-8"))
    persisted_profiles = list((persisted.get("mossen.profiles") or {}).keys())

    ok = (
        rc1 == 1 and "apiKey" in stderr1
        and rc2 == 1 and "ghost" in stderr2
        and rc3 == 1 and "ghost" in stderr3
        and rc4 == 1 and parsed4.get("deleted") is False
        and rc5a == 0
        and rc5b == 1 and "already exists" in stderr5b
        and persisted_profiles == ["y"]
    )

    write_command_log(
        ctx,
        [RUN_MOSSEN, "<validation 5 negative cases>"],
        f"persisted={persisted_profiles}",
        f"rc1={rc1} rc2={rc2} rc3={rc3} rc4={rc4} rc5a={rc5a} rc5b={rc5b}",
        rc1 + rc2 + rc3 + rc4 + rc5a + rc5b,
    )

    return {
        "name": "M9_6_profile_cli_validation_failures",
        "ok": ok,
        "rcs": [rc1, rc2, rc3, rc4, rc5a, rc5b],
        "delete_ghost_payload": parsed4,
        "persisted_profiles_after_failures": persisted_profiles,
        "_ctx": ctx,
    }


def main() -> int:
    cases = [
        case_full_lifecycle(),
        case_validation_failures(),
    ]
    summary_status = "passed" if all(c.get("ok") for c in cases) else "failed"

    last_ctx = cases[-1].pop("_ctx")
    for c in cases[:-1]:
        c.pop("_ctx", None)

    write_assertions(
        last_ctx,
        status=summary_status,
        assertions=[
            {
                "name": c["name"],
                "expected": True,
                "actual": c.get("ok"),
                "passed": c.get("ok"),
                "evidence": json.dumps(
                    {k: v for k, v in c.items() if k not in ("step_summary",)},
                    ensure_ascii=False,
                )[:500],
            }
            for c in cases
        ],
    )
    print(json.dumps({"status": summary_status, "results": cases}, indent=2, ensure_ascii=False))
    return 0 if summary_status == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
