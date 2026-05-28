#!/usr/bin/env python3

import argparse
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import time
import shutil
import textwrap
import unicodedata
from pathlib import Path
from typing import Any, Callable, Optional

import smoke_check as smoke


ROOT = Path(__file__).resolve().parents[1]
CLI = str(ROOT / "run-mossen.sh")
CLI_DIRECT = str(ROOT / "run-bun-featured.sh")
CLI_ENTRYPOINT = str(ROOT / "entrypoints" / "cli.tsx")
RUN_BUN = str(ROOT / "run-bun-featured.sh")
PERSONAL_ACCEPTANCE_PID_FILE = ROOT / ".mossensrc" / "personal-acceptance.pid"
MOSSEN_CONFIG_ENV_KEY = "MOSSEN_CONFIG_DIR"


def reap_stale_personal_acceptance_processes() -> None:
    current_pid = os.getpid()
    proc = subprocess.run(
        ["ps", "-Ao", "pid=,command="],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=True,
    )
    stale_pids: list[int] = []
    for raw_line in proc.stdout.splitlines():
        line = raw_line.strip()
        if not line or "scripts/personal_acceptance_check.py" not in line:
            continue
        pid_text, _, command = line.partition(" ")
        if "ps -Ao pid=,command=" in command:
            continue
        try:
            pid = int(pid_text)
        except ValueError:
            continue
        if pid != current_pid:
            stale_pids.append(pid)

    for pid in stale_pids:
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            continue
    if stale_pids:
        time.sleep(0.5)
    for pid in stale_pids:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            continue


def register_personal_acceptance_pid() -> None:
    PERSONAL_ACCEPTANCE_PID_FILE.parent.mkdir(parents=True, exist_ok=True)
    PERSONAL_ACCEPTANCE_PID_FILE.write_text(str(os.getpid()))


def unregister_personal_acceptance_pid() -> None:
    if not PERSONAL_ACCEPTANCE_PID_FILE.exists():
        return
    try:
        pid_text = PERSONAL_ACCEPTANCE_PID_FILE.read_text().strip()
    except OSError:
        return
    if pid_text == str(os.getpid()):
        PERSONAL_ACCEPTANCE_PID_FILE.unlink(missing_ok=True)


SAFE_SLASH_UI_PROBE_SPECS: dict[str, dict[str, object]] = {
    "/config": {
        "patterns": [
            "Config",
            "配置",
            "Language",
            "语言",
            "Output style",
            "输出风格",
            "Settings",
            "设置",
        ],
        "timeout": 50.0,
    },
    "/help": {
        "patterns": [
            "Help",
            "帮助",
            "Available commands",
            "可用命令",
            "Mossen",
        ],
        "timeout": 50.0,
    },
    "/model": {
        "patterns": [
            "Switch between available models",
            "切换可用模型",
            "future Mossen sessions",
            "未来的编码助手会话",
            "--model",
            "Model",
            "模型",
        ],
        "timeout": 50.0,
    },
    "/memory": {
        "patterns": [
            "Memory",
            "记忆",
            "MOSSEN.md",
            "No memory",
            "未找到记忆",
        ],
        "timeout": 50.0,
    },
    "/status": {
        "patterns": [
            "Status",
            "状态",
            "Session ID:",
            "会话 ID",
            "Backend URL:",
            "Language",
            "语言",
            "Context window",
            "Model tier",
            "Execution profile",
            "Reasoning profile",
            "Context pressure",
            "Auto-compact",
            "Platform Core",
            "平台核心",
            "Local git",
            "本地 git",
            "MCP:",
            "Security:",
            "安全",
            "Agents:",
            "Plugins:",
        ],
        "timeout": 50.0,
    },
    "/statusline": {
        "patterns": [
            "statusLine",
            "Status line",
            "状态栏",
            "No statusLine is configured",
            "未配置 statusLine",
            "Current statusLine setup",
        ],
        "timeout": 50.0,
    },
    "/stats": {
        "patterns": [
            "All time",
            "所有时间",
            "Last 7 days",
            "最近 7 天",
            "Sessions",
            "会话",
            "Tokens per Day",
            "每天 Tokens",
            "Loading your Mossen stats",
            "正在加载",
        ],
        "timeout": 60.0,
    },
    "/skills": {
        "patterns": [
            "Skills",
            "技能",
            "No skills found",
            "未找到技能",
            "Built-in",
            "内置",
        ],
        "timeout": 50.0,
    },
    "/permissions": {
        "patterns": [
            "Permissions",
            "权限",
            "permission",
            "bypass",
            "mode",
        ],
        "timeout": 50.0,
    },
    "/hooks": {
        "patterns": [
            "Hooks",
            "钩子",
            "configured",
            "已配置",
            "Learn more",
            "了解更多",
            "No hooks configured for this event.",
        ],
        "timeout": 50.0,
    },
    "/tasks": {
        "patterns": [
            "Background tasks",
            "后台任务",
            "No tasks currently running",
            "当前没有运行中的任务",
            "Shells",
            "Local agents",
            "Agents",
        ],
        "timeout": 50.0,
    },
    "/theme": {
        "patterns": [
            "Theme",
            "主题",
            "Dark mode",
            "深色模式",
            "Light mode",
            "浅色模式",
            "Syntax highlighting",
            "语法高亮",
        ],
        "timeout": 50.0,
    },
    "/color": {
        "patterns": [
            "Color",
            "颜色",
            "Agent color",
            "Choose",
            "选择",
            "Please provide a color.",
            "Available colors:",
            "Session color set to:",
            "Session color reset to default",
        ],
        "timeout": 50.0,
    },
    "/plugin": {
        "patterns": [
            "Plugins",
            "插件",
            "Discover",
            "Installed",
            "Marketplaces",
            "Errors",
            "Plugin Command Usage",
            "Manage installed plugins",
            "管理已安装插件",
            "Browse and install plugins",
            "浏览并安装插件",
            "Select marketplace",
            "选择市场",
            "Installed plugins",
            "已安装插件",
        ],
        "timeout": 50.0,
    },
    "/sandbox": {
        "patterns": [
            "Sandbox:",
            "沙箱",
            "Sandbox is not enabled",
            "No Sandbox",
            "Auto-allow mode:",
            "Overrides",
        ],
        "timeout": 50.0,
    },
    "/agents": {
        "patterns": [
            "Built-in agents",
            "Built-in agents (always available)",
            "内置代理",
            "No agents found",
            "未找到任何 agent",
            "Create specialized subagents",
            "Project (.mossen/agents/)",
            "Personal (~/.mossen/agents/)",
        ],
        "timeout": 50.0,
    },
    "/mcp": {
        "patterns": [
            "Manage MCP servers",
            "管理 MCP 服务器",
            "Project MCPs",
            "User MCPs",
            "Local MCPs",
            "Built-in MCPs",
            "No MCP servers configured",
            "当前未配置任何 MCP 服务器",
            "mossen mcp --help",
        ],
        "timeout": 50.0,
    },
    "/doctor": {
        "patterns": [
            "Doctor",
            "诊断信息",
            "Platform Core",
            "平台核心",
            "Checking",
            "正在检查",
        ],
        "timeout": 50.0,
    },
    "/plan": {
        "patterns": [
            "Enter plan mode?",
            "进入规划模式？",
            "wants to enter plan mode",
            "想进入规划模式",
            "No code changes will be made until you approve the plan.",
            "在你确认方案之前，不会进行任何代码改动。",
            "Enabled plan mode",
            "已启用规划模式",
            "规划模式已开启",
            "Current Plan",
            "当前计划",
            "Already in plan mode",
            "当前已处于规划模式",
            "Yes, enter plan mode",
            "是，进入规划模式",
            "No, start implementing now",
            "否，直接开始实现",
        ],
        "timeout": 75.0,
        "retries": 3,
    },
    "/lang": {
        "patterns": [
            "Language switched to English",
            "已切换为中文",
            "当前偏好",
            "Preference:",
            "当前生效",
            "Active:",
        ],
        "timeout": 50.0,
    },
    "/effort": {
        "patterns": [
            "Effort level:",
            "Current effort level:",
            "当前 effort 级别",
            "effort 级别：",
            "Invalid argument:",
        ],
        "timeout": 50.0,
    },
    "/profile": {
        "patterns": [
            "Current execution profile:",
            "Current reasoning profile:",
            "Mapped effort level:",
            "当前执行配置：",
            "当前推理配置：",
            "映射后的 effort 级别：",
        ],
        "timeout": 50.0,
    },
    "/context": {
        "patterns": [
            "System prompt",
            "Messages",
            "Free space:",
            "Auto-compact:",
            "Recent compact:",
            "tokens",
        ],
        "timeout": 75.0,
    },
    "/cost": {
        "patterns": [
            "Total cost:",
            "Total duration (API):",
            "Total code changes:",
        ],
        "timeout": 50.0,
    },
    "/feedback": {
        "patterns": [
            "Describe the issue below:",
            "请在下方描述问题：",
            "We will use your feedback",
            "我们会使用你的反馈",
            "Feedback ID:",
            "反馈 ID：",
            "Feedback is not configured for this personal build",
            "此个人版未配置反馈端点",
        ],
        "timeout": 50.0,
    },
    "/output-style": {
        "patterns": [
            "/output-style has been deprecated.",
            "Use /config to change your output style",
            "Changes take effect on the next session.",
        ],
        "timeout": 50.0,
    },
}

PROMPT_COMMAND_NAMES = {
    "batch",
    "debug",
    "dws",
    "init",
    "insights",
    "loop",
    "pr-comments",
    "review",
    "security-review",
    "simplify",
    "update-config",
}

PROBED_SLASH_COMMAND_NAMES = {
    "agents",
    "config",
    "doctor",
    "help",
    "hooks",
    "mcp",
    "memory",
    "model",
    "permissions",
    "plan",
    "plugin",
    "sandbox",
    "skills",
    "stats",
    "status",
    "statusline",
    "tasks",
    "theme",
    "effort",
    "profile",
    "context",
    "cost",
    "feedback",
    "color",
}

HOSTED_OR_EXTERNAL_COMMAND_NAMES = {
    "assistant",
    "desktop",
    "fast",
    "install-github-app",
    "install-slack-app",
    "login",
    "logout",
    "mobile",
    "passes",
    "proactive",
    "privacy-settings",
    "remote-env",
    "ultrareview",
    "upgrade",
    "usage",
    "voice",
}

STATEFUL_LOCAL_COMMAND_NAMES = {
    "add-dir",
    "branch",
    "btw",
    "clear",
    "compact",
    "copy",
    "diff",
    "exit",
    "export",
    "extra-usage",
    "heapdump",
    "ide",
    "keybindings",
    "lang",
    "output-style",
    "rate-limit-options",
    "release-notes",
    "reload-plugins",
    "rename",
    "resume",
    "rewind",
    "stickers",
    "terminal-setup",
    "vim",
}

CONDITIONALLY_HIDDEN_SLASH_COMMAND_NAMES = {
    # These commands are real source-level surfaces, but they are intentionally
    # hidden when the current auth/provider/build flags do not support them.
    "desktop",
    "extra-usage",
    "fast",
    "install-github-app",
    "install-slack-app",
    "keybindings",
    "privacy-settings",
    "rate-limit-options",
    "remote-env",
    "ultrareview",
    "upgrade",
    "usage",
    # Deferred personal-edition surfaces. Keep source visible to tests, but
    # hide them from the personal slash command inventory until explicitly
    # re-enabled.
    "assistant",
    "heapdump",
    "output-style",
    "passes",
    "pr-comments",
    "proactive",
    "release-notes",
    "stickers",
    "voice",
    # Historical prompt surface kept in the acceptance inventory so a return is
    # noticed, but it is not part of the current personal-edition command set.
    "dws",
}

KNOWN_SLASH_COMMAND_NAMES = (
    PROMPT_COMMAND_NAMES
    | PROBED_SLASH_COMMAND_NAMES
    | HOSTED_OR_EXTERNAL_COMMAND_NAMES
    | STATEFUL_LOCAL_COMMAND_NAMES
)


def compact_text(text: str) -> str:
    normalized = unicodedata.normalize("NFKC", smoke.normalize_output(text))
    normalized = "".join(
        char for char in normalized if not unicodedata.category(char).startswith("C")
    )
    return re.sub(r"\s+", "", normalized)


def has_current_selector_entry(compacted: str) -> bool:
    return bool(
        re.search(r"\(curr[a-z]{2,5}\)", compacted)
        or "❯(current)" in compacted
        or "❯(curret)" in compacted
    )


def run_auth_status_text_smoke() -> dict[str, object]:
    proc = smoke.run_cmd_full([CLI, "auth", "status", "--text"], timeout=60)
    output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
    if proc.returncode != 0:
        raise RuntimeError(f"auth status --text failed\n--- output ---\n{output[:1200]}")
    if "Platform core:" not in output or (
        "Provider:" not in output and "Login method:" not in output
    ):
        raise RuntimeError(
            "auth status --text missing expected runtime summary\n--- output ---\n"
            + output[:1200]
        )
    if "Model tier:" not in output:
        raise RuntimeError(
            "auth status --text missing model tier\n--- output ---\n" + output[:1200]
        )
    if "Context window:" not in output:
        raise RuntimeError(
            "auth status --text missing context window\n--- output ---\n"
            + output[:1200]
        )
    if "Language:" not in output:
        raise RuntimeError(
            "auth status --text missing language\n--- output ---\n" + output[:1200]
        )
    if "Execution profile:" not in output or "Reasoning profile:" not in output:
        raise RuntimeError(
            "auth status --text missing profile summary\n--- output ---\n"
            + output[:1200]
        )
    lines = [line for line in output.splitlines() if line.strip()]
    return {
        "status": "ok",
        "summary": lines[:10],
    }


def run_platform_check_json() -> dict[str, object]:
    raw = smoke.run_cmd([RUN_BUN, "scripts/platform_check.ts"], timeout=120)
    parsed = json.loads(raw)
    if not isinstance(parsed, dict) or "runtime" not in parsed:
        raise RuntimeError(f"unexpected platform_check output\n--- output ---\n{raw[:1200]}")
    provider = (((parsed.get("runtime") or {}).get("provider")) or {})
    if provider.get("tier") not in {"local", "cloud"}:
        raise RuntimeError(
            f"platform_check missing provider.tier\n--- output ---\n{raw[:1200]}"
        )
    return parsed


def run_smoke_audit_in_subprocess(
    function_name: str,
    *,
    timeout: int = 240,
) -> dict[str, object]:
    env = os.environ.copy()
    scripts_path = str(ROOT / "scripts")
    existing_pythonpath = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        f"{scripts_path}{os.pathsep}{existing_pythonpath}"
        if existing_pythonpath
        else scripts_path
    )
    code = textwrap.dedent(
        f"""
        import json
        import smoke_check as smoke

        result = getattr(smoke, {function_name!r})()
        print(json.dumps(result, ensure_ascii=False))
        """
    )
    try:
        proc = subprocess.run(
            [sys.executable, "-u", "-c", code],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(f"{function_name} timed out after {timeout}s") from exc

    combined = f"{proc.stdout}\n{proc.stderr}"
    if proc.returncode != 0:
        if "Unable to connect" in combined:
            raise RuntimeError(
                f"{function_name} failed with provider transport instability: "
                f"{combined.strip()}"
            )
        raise RuntimeError(
            f"{function_name} failed with code {proc.returncode}: {combined.strip()}"
        )

    lines = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
    if not lines:
        raise RuntimeError(f"{function_name} produced no JSON output")
    return json.loads(lines[-1])


def run_self_audit_in_subprocess(
    function_name: str,
    *,
    timeout: int = 240,
) -> dict[str, object]:
    env = os.environ.copy()
    scripts_path = str(ROOT / "scripts")
    existing_pythonpath = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        f"{scripts_path}{os.pathsep}{existing_pythonpath}"
        if existing_pythonpath
        else scripts_path
    )
    code = textwrap.dedent(
        f"""
        import json
        import personal_acceptance_check as pac

        result = getattr(pac, {function_name!r})()
        print(json.dumps(result, ensure_ascii=False))
        """
    )
    try:
        proc = subprocess.run(
            [sys.executable, "-u", "-c", code],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(f"{function_name} timed out after {timeout}s") from exc

    combined = f"{proc.stdout}\n{proc.stderr}"
    if proc.returncode != 0:
        if "Unable to connect" in combined:
            raise RuntimeError(
                f"{function_name} failed with provider transport instability: "
                f"{combined.strip()}"
            )
        raise RuntimeError(
            f"{function_name} failed with code {proc.returncode}: {combined.strip()}"
        )

    lines = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
    if not lines:
        raise RuntimeError(f"{function_name} produced no JSON output")
    return json.loads(lines[-1])


def run_self_audit_in_subprocess(
    function_name: str,
    *,
    timeout: int = 240,
) -> dict[str, object]:
    env = os.environ.copy()
    scripts_path = str(ROOT / "scripts")
    existing_pythonpath = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        f"{scripts_path}{os.pathsep}{existing_pythonpath}"
        if existing_pythonpath
        else scripts_path
    )
    code = textwrap.dedent(
        f"""
        import json
        import personal_acceptance_check as pac

        result = getattr(pac, {function_name!r})()
        print(json.dumps(result, ensure_ascii=False))
        """
    )
    try:
        proc = subprocess.run(
            [sys.executable, "-u", "-c", code],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(f"{function_name} timed out after {timeout}s") from exc

    combined = f"{proc.stdout}\n{proc.stderr}"
    if proc.returncode != 0:
        if "Unable to connect" in combined:
            raise RuntimeError(
                f"{function_name} failed with provider transport instability: "
                f"{combined.strip()}"
            )
        raise RuntimeError(
            f"{function_name} failed with code {proc.returncode}: {combined.strip()}"
        )

    lines = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
    if not lines:
        raise RuntimeError(f"{function_name} produced no JSON output")
    return json.loads(lines[-1])


def run_smoke_audit_in_subprocess(
    function_name: str,
    *,
    timeout: int = 240,
) -> dict[str, object]:
    env = os.environ.copy()
    scripts_path = str(ROOT / "scripts")
    existing_pythonpath = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        f"{scripts_path}{os.pathsep}{existing_pythonpath}"
        if existing_pythonpath
        else scripts_path
    )
    code = textwrap.dedent(
        f"""
        import json
        import smoke_check as smoke

        result = getattr(smoke, {function_name!r})()
        print(json.dumps(result, ensure_ascii=False))
        """
    )
    try:
        proc = subprocess.run(
            [sys.executable, "-u", "-c", code],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(f"{function_name} timed out after {timeout}s") from exc

    combined = f"{proc.stdout}\n{proc.stderr}"
    if proc.returncode != 0:
        if "Unable to connect" in combined:
            raise RuntimeError(
                f"{function_name} failed with provider transport instability: "
                f"{combined.strip()}"
            )
        raise RuntimeError(
            f"{function_name} failed with code {proc.returncode}: {combined.strip()}"
        )

    lines = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
    if not lines:
        raise RuntimeError(f"{function_name} produced no JSON output")
    return json.loads(lines[-1])


def run_prompt_smoke() -> dict[str, object]:
    phrase = "personal acceptance ok"
    output = smoke.run_cmd(
        [CLI, "-p", f"Reply with exactly: {phrase}"],
        timeout=90,
    )
    smoke.expect_contains(output, phrase, "personal acceptance prompt")
    return {"status": "ok", "reply": output.strip()}


def run_tool_use_smoke() -> dict[str, object]:
    phrase = "personal tool ok"
    prompt = (
        "Use Bash exactly once to run: printf 'tool ok\\n'. "
        f"Then reply with exactly: {phrase}"
    )
    combined = ""
    last_timeout: subprocess.TimeoutExpired | None = None
    for timeout in (90, 180):
        try:
            proc = run_cli_capture(
                [
                    CLI,
                    "--dangerously-skip-permissions",
                    "-p",
                    "--verbose",
                    "--output-format",
                    "stream-json",
                    "--max-turns",
                    "6",
                    prompt,
                ],
                ROOT,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired as exc:
            last_timeout = exc
            time.sleep(1.0)
            continue

        combined = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
        if proc.returncode != 0:
            raise RuntimeError(
                f"tool use smoke failed with code {proc.returncode}\n--- output ---\n{combined[:4000]}"
            )

        events = parse_stream_json_events(proc.stdout or "")
        tool_blocks = collect_tool_use_blocks(events)
        bash_blocks = [
            block
            for block in tool_blocks
            if isinstance(block, dict) and block.get("name") == "Bash"
        ]
        assistant_texts: list[str] = []
        for event in events:
            if event.get("type") != "assistant":
                continue
            message = event.get("message")
            if not isinstance(message, dict):
                continue
            content = message.get("content")
            if not isinstance(content, list):
                continue
            for block in content:
                if (
                    isinstance(block, dict)
                    and block.get("type") == "text"
                    and isinstance(block.get("text"), str)
                ):
                    assistant_texts.append(block["text"])

        if len(bash_blocks) != 1:
            raise RuntimeError(
                "tool use smoke expected exactly one Bash tool call\n"
                f"bash_calls={len(bash_blocks)}\n--- output ---\n{combined[:4000]}"
            )
        if not any(phrase in text for text in assistant_texts):
            raise RuntimeError(
                "tool use smoke did not emit the expected final phrase\n"
                f"assistant_texts={assistant_texts}\n--- output ---\n{combined[:4000]}"
            )

        return {
            "status": "ok",
            "reply": phrase,
            "bashCalls": len(bash_blocks),
            "assistantTextCount": len(assistant_texts),
        }

    raise RuntimeError(f"tool use smoke timed out twice: {last_timeout}")


def wait_for_cli_prompt(fd: int, timeout: float = 25) -> None:
    wait_for_cli_ready(fd, timeout)


def wait_for_cli_ready(fd: int, timeout: float = 25) -> dict[str, object]:
    deadline = time.time() + timeout
    data = ""
    approved_external_imports = False
    while time.time() < deadline:
        remaining = max(0.1, deadline - time.time())
        readable, _, _ = smoke.select.select([fd], [], [], remaining)
        if not readable:
            continue
        try:
            chunk = os.read(fd, 4096).decode("utf-8", errors="ignore")
        except OSError:
            break
        if not chunk:
            break
        smoke.respond_to_terminal_queries(fd, chunk)
        data += chunk
        normalized = smoke.normalize_output(data)
        compacted = compact_text(data)
        if (
            "Quicksafetycheck" in compacted
            or "Yes,Itrustthisfolder" in compacted
            or "快速安全检查" in compacted
            or "是，我信任这个文件夹" in compacted
        ):
            smoke.write_line(fd, "\r")
            data = ""
            continue
        if (
            "AllowexternalMOSSEN.mdfileimports?" in compacted
            or "Yes,allowexternalimports" in compacted
        ):
            smoke.write_line(fd, "\r")
            approved_external_imports = True
            data = ""
            continue
        if "❯" in normalized:
            return {
                "status": "ok",
                "approved_external_imports": approved_external_imports,
            }
    raise TimeoutError(
        "Timed out waiting for interactive CLI prompt\n--- output ---\n" + data
    )


def spawn_cli_custom(
    *,
    args: Optional[list[str]] = None,
    cwd: Path = ROOT,
    env_overrides: Optional[dict[str, str]] = None,
) -> tuple[subprocess.Popen[bytes], int]:
    master_fd, slave_fd = smoke.pty.openpty()
    argv = [CLI_DIRECT, CLI_ENTRYPOINT, *(args or [])]
    env = os.environ.copy()
    if env_overrides:
        env.update(env_overrides)
    proc = subprocess.Popen(
        argv,
        cwd=cwd,
        env=env,
        stdin=slave_fd,
        stdout=slave_fd,
        stderr=slave_fd,
        close_fds=True,
        start_new_session=True,
    )
    os.close(slave_fd)
    smoke.register_smoke_pid(proc.pid)
    return proc, master_fd


def mossen_config_env(
    config: Path | str, extra: Optional[dict[str, str]] = None
) -> dict[str, str]:
    env = {MOSSEN_CONFIG_ENV_KEY: str(config)}
    if extra:
        env.update(extra)
    return env


def wait_for_cli_patterns(
    fd: int,
    command: str,
    patterns: list[object],
    timeout: float,
) -> tuple[str, str]:
    deadline = time.time() + timeout
    data = ""
    matched: Optional[str] = None
    pattern_groups: list[tuple[str, list[str]]] = []
    for pattern in patterns:
        if isinstance(pattern, (list, tuple, set)):
            variants = [str(variant) for variant in pattern if str(variant)]
            raw = variants[0] if variants else ""
        else:
            raw = str(pattern)
            variants = [raw] if raw else []
        pattern_groups.append((raw, [compact_text(variant) for variant in variants]))
    while time.time() < deadline:
        remaining = max(0.1, deadline - time.time())
        readable, _, _ = smoke.select.select([fd], [], [], remaining)
        if not readable:
            continue
        try:
            chunk = os.read(fd, 4096).decode("utf-8", errors="ignore")
        except OSError:
            break
        if not chunk:
            break
        smoke.respond_to_terminal_queries(fd, chunk)
        data += chunk
        compact = compact_text(data)
        for raw, variants in pattern_groups:
            if any(variant and variant in compact for variant in variants):
                matched = raw
                break
        if matched:
            return data, matched
    compact = compact_text(data)
    for raw, variants in pattern_groups:
        if any(variant and variant in compact for variant in variants):
            return data, raw
    raise TimeoutError(f"Timed out waiting for {command} UI\n--- output ---\n{data}")


def wait_for_cli_all_patterns(
    fd: int,
    command: str,
    patterns: list[object],
    timeout: float,
) -> tuple[str, list[str]]:
    deadline = time.time() + timeout
    data = ""
    pattern_groups: list[tuple[str, list[str]]] = []
    for pattern in patterns:
        if isinstance(pattern, (list, tuple, set)):
            variants = [str(variant) for variant in pattern if str(variant)]
            raw = variants[0] if variants else ""
        else:
            raw = str(pattern)
            variants = [raw] if raw else []
        pattern_groups.append((raw, [compact_text(variant) for variant in variants]))
    seen: set[str] = set()
    while time.time() < deadline:
        remaining = max(0.1, deadline - time.time())
        readable, _, _ = smoke.select.select([fd], [], [], remaining)
        if not readable:
            continue
        try:
            chunk = os.read(fd, 4096).decode("utf-8", errors="ignore")
        except OSError:
            break
        if not chunk:
            break
        smoke.respond_to_terminal_queries(fd, chunk)
        data += chunk
        compact = compact_text(data)
        for raw, variants in pattern_groups:
            if any(variant and variant in compact for variant in variants):
                seen.add(raw)
        if len(seen) == len(patterns):
            return data, [raw for raw, _variants in pattern_groups]
    missing = [raw for raw, _variants in pattern_groups if raw not in seen]
    raise TimeoutError(
        f"Timed out waiting for {command} UI missing {missing}\n--- output ---\n{data}"
    )


def with_cli_probe_retry(
    fn: Callable[[], dict[str, object]],
    *,
    attempts: int = 2,
) -> dict[str, object]:
    last_error: Exception | None = None
    for _attempt in range(attempts):
        try:
            return fn()
        except Exception as exc:
            last_error = exc
            time.sleep(0.2)
    assert last_error is not None
    raise last_error


def load_slash_command_inventory() -> list[dict[str, object]]:
    proc = subprocess.run(
        [
            "bun",
            "-e",
            (
                "import { enableConfigs } from './utils/config.ts'; "
                "import { initBundledSkills } from './skills/bundled/index.ts'; "
                "import { getCommands } from './commands.ts'; "
                "enableConfigs(); "
                "initBundledSkills(); "
                "const cmds = await getCommands(process.cwd()); "
                "const view = cmds.map(c => ({"
                "name: c.name,"
                "type: c.type,"
                "aliases: c.aliases || [],"
                "availability: c.availability || null,"
                "description: typeof c.description === 'string' ? c.description : ''"
                "})); "
                "console.log(JSON.stringify(view));"
            ),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=60,
        check=True,
    )
    payload = json.loads(smoke.normalize_output(proc.stdout.strip()))
    if not isinstance(payload, list) or not payload:
        raise RuntimeError(f"slash inventory returned unexpected payload\n--- output ---\n{proc.stdout[:1200]}")
    return payload


def classify_slash_command(command: dict[str, object]) -> Optional[str]:
    name = str(command["name"])
    if name in PROBED_SLASH_COMMAND_NAMES:
        return "ui_probe"
    if name in HOSTED_OR_EXTERNAL_COMMAND_NAMES:
        return "hosted_or_external"
    if name in PROMPT_COMMAND_NAMES:
        return "prompt"
    if name in STATEFUL_LOCAL_COMMAND_NAMES:
        return "stateful_local"
    return None


def run_slash_inventory_audit() -> dict[str, object]:
    inventory = load_slash_command_inventory()
    names = [str(command["name"]) for command in inventory]
    if len(names) != len(set(names)):
        duplicates = sorted(name for name in set(names) if names.count(name) > 1)
        raise RuntimeError(f"duplicate slash command names detected: {duplicates}")

    inventory_name_set = set(names)
    missing = sorted(KNOWN_SLASH_COMMAND_NAMES - inventory_name_set)
    missing_required = sorted(
        set(missing) - CONDITIONALLY_HIDDEN_SLASH_COMMAND_NAMES
    )
    unexpected = sorted(inventory_name_set - KNOWN_SLASH_COMMAND_NAMES)
    if missing_required or unexpected:
        raise RuntimeError(
            "slash inventory drifted\n"
            f"missing_required={missing_required}\n"
            f"missing_conditionally_hidden={sorted(set(missing) & CONDITIONALLY_HIDDEN_SLASH_COMMAND_NAMES)}\n"
            f"unexpected={unexpected}"
        )

    prompt_inventory = {name for name, command in zip(names, inventory) if command["type"] == "prompt"}
    expected_visible_prompts = PROMPT_COMMAND_NAMES - CONDITIONALLY_HIDDEN_SLASH_COMMAND_NAMES
    missing_prompt_commands = sorted(expected_visible_prompts - prompt_inventory)
    unexpected_prompt_commands = sorted(prompt_inventory - PROMPT_COMMAND_NAMES)
    if missing_prompt_commands or unexpected_prompt_commands:
        raise RuntimeError(
            "prompt slash command inventory drifted\n"
            f"missing={missing_prompt_commands}\n"
            f"unexpected={unexpected_prompt_commands}\n"
            f"actual={sorted(prompt_inventory)}"
        )

    alias_owner: dict[str, str] = {}
    alias_collisions: list[str] = []
    for command in inventory:
        name = str(command["name"])
        aliases = command.get("aliases") or []
        if not isinstance(aliases, list):
            raise RuntimeError(f"slash aliases for {name} are not a list")
        for alias in aliases:
            alias_str = str(alias)
            existing = alias_owner.get(alias_str)
            if existing and existing != name:
                alias_collisions.append(f"{alias_str}:{existing}|{name}")
            alias_owner[alias_str] = name
            if alias_str in inventory_name_set and alias_str != name:
                alias_collisions.append(f"{alias_str}:alias-collides-with-command-name")
    if alias_collisions:
        raise RuntimeError(f"slash alias collisions detected: {sorted(set(alias_collisions))}")

    unclassified = sorted(
        name for command in inventory if (name := str(command["name"])) and classify_slash_command(command) is None
    )
    if unclassified:
        raise RuntimeError(f"slash inventory has unclassified commands: {unclassified}")

    category_counts = {
        "ui_probe": 0,
        "hosted_or_external": 0,
        "prompt": 0,
        "stateful_local": 0,
    }
    type_counts: dict[str, int] = {}
    for command in inventory:
        category = classify_slash_command(command)
        category_counts[str(category)] += 1
        cmd_type = str(command["type"])
        type_counts[cmd_type] = type_counts.get(cmd_type, 0) + 1

    return {
        "status": "ok",
        "total_commands": len(inventory),
        "type_counts": type_counts,
        "category_counts": category_counts,
        "ui_probe_commands": sorted(PROBED_SLASH_COMMAND_NAMES),
        "hosted_or_external_commands": sorted(HOSTED_OR_EXTERNAL_COMMAND_NAMES),
        "prompt_commands": sorted(PROMPT_COMMAND_NAMES),
        "stateful_local_commands": sorted(STATEFUL_LOCAL_COMMAND_NAMES),
        "conditionally_hidden_commands": sorted(
            set(missing) & CONDITIONALLY_HIDDEN_SLASH_COMMAND_NAMES
        ),
    }


def run_slash_ui_probe(command: str) -> dict[str, object]:
    spec = SAFE_SLASH_UI_PROBE_SPECS[command]
    last_error: Exception | None = None
    retries = int(spec.get("retries", 2))
    for _attempt in range(retries):
        try:
            return run_dialog_command_smoke(
                command,
                list(spec["patterns"]),
                timeout=float(spec["timeout"]),
            )
        except Exception as exc:
            last_error = exc
            time.sleep(0.2)
    assert last_error is not None
    raise last_error


def run_command_result_smoke(
    command: str,
    patterns: list[str],
    timeout: float = 40,
) -> dict[str, object]:
    proc, fd = smoke.spawn_cli()
    try:
        wait_for_cli_prompt(fd, min(max(25.0, timeout), 35.0))
        last_error: Exception | None = None
        for attempt in range(2):
            if attempt == 0:
                time.sleep(0.2)
            smoke.write_line(fd, f"{command}\n")
            try:
                _data, matched = wait_for_cli_patterns(fd, command, patterns, timeout)
                return {"status": "ok", "matched": matched, "attempts": attempt + 1}
            except Exception as exc:
                last_error = exc
                try:
                    wait_for_cli_prompt(fd, 3)
                except Exception:
                    break
        assert last_error is not None
        raise last_error
    finally:
        try:
            os.close(fd)
        except OSError:
            pass
        smoke.terminate_process_tree(proc)


def run_dialog_command_smoke(
    command: str,
    patterns: list[str],
    timeout: float = 40,
) -> dict[str, object]:
    proc, fd = smoke.spawn_cli()
    try:
        wait_for_cli_prompt(fd, min(max(25.0, timeout), 35.0))
        last_error: Exception | None = None
        for attempt in range(2):
            if attempt == 0:
                time.sleep(0.2)
            smoke.write_line(fd, f"{command}\n")
            try:
                _data, matched = wait_for_cli_patterns(fd, command, patterns, timeout)
                return {"status": "ok", "matched": matched, "attempts": attempt + 1}
            except Exception as exc:
                last_error = exc
                try:
                    wait_for_cli_prompt(fd, 3)
                except Exception:
                    break
        assert last_error is not None
        raise last_error
    finally:
        try:
            os.close(fd)
        except OSError:
            pass
        smoke.terminate_process_tree(proc)


def run_context_observability_smoke() -> dict[str, object]:
    patterns = [
        "Context Usage",
        "Auto-compact:",
        "Recent compact:",
    ]
    last_error: Exception | None = None
    for attempt in range(2):
        proc, fd = smoke.spawn_cli()
        try:
            wait_for_cli_prompt(fd, 25)
            smoke.write_line(fd, "/context\n")
            _data, matched = wait_for_cli_all_patterns(
                fd,
                "/context observability",
                patterns,
                75,
            )
            return {
                "status": "ok",
                "matched": matched,
                "attempts": attempt + 1,
            }
        except Exception as exc:
            last_error = exc
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)
    assert last_error is not None
    raise last_error


def run_status_observability_smoke() -> dict[str, object]:
    proc, fd = smoke.spawn_cli()
    patterns = [
        "Context pressure:",
        "Auto-compact:",
        "Recent compact:",
    ]
    try:
        wait_for_cli_prompt(fd, 25)
        smoke.write_line(fd, "/status\n")
        _data, matched = wait_for_cli_all_patterns(
            fd,
            "/status observability",
            patterns,
            50,
        )
        return {"status": "ok", "matched": matched}
    finally:
        try:
            os.close(fd)
        except OSError:
            pass
        smoke.terminate_process_tree(proc)


def run_status_profile_surface_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-status-profile-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)

        settings_path = config / "settings.json"
        settings: dict[str, Any] = {}
        if settings_path.exists():
            settings = json.loads(settings_path.read_text())
        settings.update(
            {
                "executionProfile": "review",
                "reasoningProfile": "deep",
                "effortLevel": "high",
            }
        )
        settings_path.write_text(
            json.dumps(settings, ensure_ascii=False, indent=2) + "\n"
        )

        proc, fd = spawn_cli_custom(env_overrides=mossen_config_env(config))
        patterns = [
            "Execution",
            "review",
            "profile:",
            "Reasoning",
            "deep",
            "Model tier",
            "cloud",
        ]
        try:
            wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "/status\n")
            _data, matched = wait_for_cli_all_patterns(
                fd,
                "/status profile surface",
                patterns,
                60,
            )
            return {
                "status": "ok",
                "matched": matched,
                "config": str(settings_path),
            }
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)


def run_status_local_tier_surface_smoke() -> dict[str, object]:
    def _probe() -> dict[str, object]:
        with tempfile.TemporaryDirectory(
            prefix="mossensrc-status-local-tier-config."
        ) as config_dir:
            config = Path(config_dir)
            seed_temp_config_settings(config)

            proc, fd = spawn_cli_custom(
                env_overrides=mossen_config_env(
                    config,
                    get_local_tier_env_overrides(),
                )
            )
            patterns = [
                ["Context window", "200,000 tokens"],
                "Model tier",
                "local",
                "Execution profile",
                "coding",
                "Reasoning profile",
                "standard",
            ]
            try:
                wait_for_cli_ready(fd, 30)
                smoke.write_line(fd, "/status\n")
                _data, matched = wait_for_cli_all_patterns(
                    fd,
                    "/status local tier surface",
                    patterns,
                    60,
                )
                return {
                    "status": "ok",
                    "matched": matched,
                }
            finally:
                try:
                    os.close(fd)
                except OSError:
                    pass
                smoke.terminate_process_tree(proc)

    return with_cli_probe_retry(_probe)


def run_auth_status_surface_consistency_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-auth-status-surface-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)

        settings_path = config / "settings.json"
        settings: dict[str, Any] = {}
        if settings_path.exists():
            settings = json.loads(settings_path.read_text())
        settings.update(
            {
                "executionProfile": "low-cost",
                "effortLevel": "high",
            }
        )
        settings_path.write_text(
            json.dumps(settings, ensure_ascii=False, indent=2) + "\n"
        )

        env_overrides = mossen_config_env(
            config,
            get_local_tier_env_overrides(),
        )

        snapshot_proc = run_cli_capture(
            [
                RUN_BUN,
                "-e",
                textwrap.dedent(
                    """\
                    import { getInitialSettings } from './utils/settings/settings.ts'
                    import { getCustomBackendObservabilitySnapshot } from './utils/status.tsx'

                    console.log(JSON.stringify(getCustomBackendObservabilitySnapshot(getInitialSettings())))
                    """
                ),
            ],
            ROOT,
            timeout=90,
            env_overrides=env_overrides,
        )
        snapshot_output = smoke.normalize_output(
            (snapshot_proc.stdout or "") + (snapshot_proc.stderr or "")
        )
        if snapshot_proc.returncode != 0:
            raise RuntimeError(
                "failed to collect custom backend observability snapshot\n--- output ---\n"
                + snapshot_output[:2000]
            )
        snapshot = json.loads(snapshot_proc.stdout.strip())

        auth_proc = run_cli_capture(
            [CLI, "auth", "status", "--text"],
            ROOT,
            timeout=90,
            env_overrides=env_overrides,
        )
        auth_output = smoke.normalize_output(
            (auth_proc.stdout or "") + (auth_proc.stderr or "")
        )
        if auth_proc.returncode != 0:
            raise RuntimeError(
                "auth status --text surface consistency failed\n--- output ---\n"
                + auth_output[:2000]
            )

        auth_required = [
            f"Login method: {snapshot['providerLabel']}",
            f"Model tier: {snapshot['modelTier']}",
            f"Language: {snapshot['interactiveLanguage']}",
            f"Execution profile: {snapshot['executionProfile']}",
            f"Reasoning profile: {snapshot['reasoningProfile']}",
            f"Context window: {snapshot['contextWindowTokens']:,} tokens",
        ]
        missing_auth = [pattern for pattern in auth_required if pattern not in auth_output]
        if missing_auth:
            raise RuntimeError(
                f"auth status --text missing shared snapshot fields: {missing_auth}\n--- output ---\n"
                + auth_output[:2400]
            )

        properties_proc = run_cli_capture(
            [
                RUN_BUN,
                "-e",
                textwrap.dedent(
                    """\
                    import { buildAPIProviderProperties } from './utils/status.tsx'

                    console.log(JSON.stringify(buildAPIProviderProperties()))
                    """
                ),
            ],
            ROOT,
            timeout=90,
            env_overrides=env_overrides,
        )
        properties_output = smoke.normalize_output(
            (properties_proc.stdout or "") + (properties_proc.stderr or "")
        )
        if properties_proc.returncode != 0:
            raise RuntimeError(
                "failed to collect /status provider properties\n--- output ---\n"
                + properties_output[:2000]
            )
        properties = json.loads(properties_proc.stdout.strip())
        rendered_properties = {
            str(item.get("label")): str(item.get("value"))
            for item in properties
            if isinstance(item, dict) and item.get("label") is not None
        }

        required_properties = {
            "API provider": snapshot["providerLabel"],
            "Model tier": snapshot["modelTier"],
            "Language": snapshot["interactiveLanguage"],
            "Backend URL": snapshot["backendUrl"],
            "Custom model": snapshot["customModel"],
            "Context window": f"{snapshot['contextWindowTokens']:,} tokens",
        }
        missing_properties = [
            f"{label}={expected}"
            for label, expected in required_properties.items()
            if rendered_properties.get(label) != expected
        ]
        if "Execution profile" not in rendered_properties or snapshot["executionProfile"] not in rendered_properties["Execution profile"]:
            missing_properties.append(
                f"Execution profile contains {snapshot['executionProfile']}"
            )
        if "Reasoning profile" not in rendered_properties or snapshot["reasoningProfile"] not in rendered_properties["Reasoning profile"]:
            missing_properties.append(
                f"Reasoning profile contains {snapshot['reasoningProfile']}"
            )
        if missing_properties:
            raise RuntimeError(
                f"/status provider properties diverged from shared snapshot: {missing_properties}\n--- properties ---\n"
                + json.dumps(properties, ensure_ascii=False, indent=2)[:2400]
            )

        return {
            "status": "ok",
            "snapshot": snapshot,
            "authMatched": auth_required,
            "statusProperties": rendered_properties,
            "config": str(settings_path),
        }


def run_statusline_surface_consistency_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-statusline-consistency-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)

        statusline_script = config / "statusline-consistency.py"
        statusline_script.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import json
                import sys

                payload = json.load(sys.stdin)
                print(
                    "tier={tier} profile={profile} reason={reason} lang={lang} ctx={ctx}".format(
                        tier=payload.get("model_tier", ""),
                        profile=(payload.get("profiles") or {}).get("execution", ""),
                        reason=(payload.get("profiles") or {}).get("reasoning", ""),
                        lang=payload.get("interactive_language", ""),
                        ctx=(payload.get("context_window") or {}).get("context_window_size", ""),
                    )
                )
                """
            )
        )
        statusline_script.chmod(0o755)

        settings_path = config / "settings.json"
        settings: dict[str, Any] = {}
        if settings_path.exists():
            settings = json.loads(settings_path.read_text())
        settings.update(
            {
                "statusLine": {
                    "type": "command",
                    "command": str(statusline_script),
                    "padding": 0,
                },
                "executionProfile": "low-cost",
                "effortLevel": "high",
                "language": "中文",
            }
        )
        settings_path.write_text(
            json.dumps(settings, ensure_ascii=False, indent=2) + "\n"
        )

        env_overrides = mossen_config_env(
            config,
            get_local_tier_env_overrides(),
        )

        snapshot_proc = run_cli_capture(
            [
                RUN_BUN,
                "-e",
                textwrap.dedent(
                    """\
                    import { getInitialSettings } from './utils/settings/settings.ts'
                    import { getCustomBackendObservabilitySnapshot } from './utils/status.tsx'

                    console.log(JSON.stringify(getCustomBackendObservabilitySnapshot(getInitialSettings())))
                    """
                ),
            ],
            ROOT,
            timeout=90,
            env_overrides=env_overrides,
        )
        snapshot_output = smoke.normalize_output(
            (snapshot_proc.stdout or "") + (snapshot_proc.stderr or "")
        )
        if snapshot_proc.returncode != 0:
            raise RuntimeError(
                "failed to collect statusline observability snapshot\n--- output ---\n"
                + snapshot_output[:2000]
            )
        snapshot = json.loads(snapshot_proc.stdout.strip())

        probe_script = textwrap.dedent(
            """\
            import { setSessionTrustAccepted } from './bootstrap/state.ts'
            import { executeStatusLineCommand } from './utils/hooks.ts'
            import { buildStatusLineObservabilityInput } from './utils/statusLineObservability.ts'
            import { getInitialSettings } from './utils/settings/settings.ts'
            import { getCustomBackendObservabilitySnapshot } from './utils/status.tsx'

            setSessionTrustAccepted(true)

            const settings = getInitialSettings()
            const snapshot = getCustomBackendObservabilitySnapshot(settings)
            const model = snapshot.customModel || 'example-large'
            const payload = {
              session_id: 'personal-acceptance-statusline-consistency',
              transcript_path: '/tmp/personal-acceptance-statusline-consistency.jsonl',
              cwd: process.cwd(),
              model: {
                id: model,
                display_name: snapshot.providerLabel,
              },
              workspace: {
                current_dir: process.cwd(),
                project_dir: process.cwd(),
                added_dirs: [],
              },
              version: 'acceptance',
              output_style: {
                name: 'default',
              },
              context_window: {
                total_input_tokens: 0,
                total_output_tokens: 0,
                context_window_size: snapshot.contextWindowTokens ?? 0,
                current_usage: null,
                used_percentage: 0,
                remaining_percentage: 100,
              },
              exceeds_200k_tokens: false,
              ...buildStatusLineObservabilityInput([], model, settings.effortLevel, settings, {
                autoCompactEnabled: false,
                modelTier: snapshot.modelTier,
              }),
            }

            const output = await executeStatusLineCommand(payload)
            if (!output) {
              throw new Error('statusline consistency runtime returned no output')
            }
            process.stdout.write(output + '\\n')
            """
        )

        proc = run_cli_capture(
            [RUN_BUN, "-e", probe_script],
            ROOT,
            timeout=120,
            env_overrides=env_overrides,
        )
        output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
        if proc.returncode != 0:
            raise RuntimeError(
                f"statusline surface consistency failed\n--- output ---\n{output[:2400]}"
            )

        required = [
            f"tier={snapshot['modelTier']}",
            f"profile={snapshot['executionProfile']}",
            f"reason={snapshot['reasoningProfile']}",
            f"lang={snapshot['interactiveLanguage']}",
            f"ctx={snapshot['contextWindowTokens']}",
        ]
        missing = [pattern for pattern in required if pattern not in output]
        if missing:
            raise RuntimeError(
                f"statusline surface consistency missing {missing}\n--- output ---\n{output[:2400]}"
            )

        return {
            "status": "ok",
            "snapshot": snapshot,
            "matched": required,
            "script": str(statusline_script),
            "config": str(settings_path),
        }


def run_worktree_status_surface_consistency_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-worktree-status-consistency-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)
        env_overrides = mossen_config_env(
            config,
            {
                "MOSSEN_CODE_USE_OPENAI_COMPATIBLE": "1",
                "MOSSEN_CODE_OPENAI_BASE_URL": "http://127.0.0.1:18080/v1",
                "MOSSEN_CODE_OPENAI_AUTH_TOKEN": "acceptance-token",
                "MOSSEN_CODE_OPENAI_MODEL": "example-large",
            },
        )

        probe_script = textwrap.dedent(
            """\
            import { restoreWorktreeSession } from './utils/worktree.ts'
            import { getInitialSettings } from './utils/settings/settings.ts'
            import {
              buildAPIProviderProperties,
              getCustomBackendObservabilitySnapshot,
            } from './utils/status.tsx'
            import { buildStatusLineObservabilityInput } from './utils/statusLineObservability.ts'
            import { getContextObservabilityItems } from './commands/context/context-noninteractive.ts'

            restoreWorktreeSession({
              originalCwd: '/tmp/original-repo',
              worktreePath: '/tmp/original-repo/.mossen/worktrees/feature-x',
              worktreeName: 'feature-x',
              worktreeBranch: 'worktree-feature-x',
              originalBranch: 'main',
              sessionId: 'personal-acceptance-worktree-consistency',
            })

            const settings = getInitialSettings()
            const snapshot = getCustomBackendObservabilitySnapshot(settings)
            const model = snapshot.customModel || 'example-large'
            const properties = buildAPIProviderProperties()
            const statusline = buildStatusLineObservabilityInput(
              [],
              model,
              settings.effortLevel,
              settings,
              {
                autoCompactEnabled: false,
                modelTier: snapshot.modelTier,
              },
            )
            const contextItems = getContextObservabilityItems({
              isAutoCompactEnabled: false,
              recentCompact: { hasBoundary: false, messagesSinceCompact: 0 },
              memoryFiles: [],
              rawMaxTokens: 1000,
            } as any)

            console.log(
              JSON.stringify({
                snapshot,
                properties,
                statusline,
                contextItems,
              })
            )
            """
        )

        proc = run_cli_capture(
            [RUN_BUN, "-e", probe_script],
            ROOT,
            timeout=120,
            env_overrides=env_overrides,
        )
        output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
        if proc.returncode != 0:
            raise RuntimeError(
                "worktree status surface consistency failed\n--- output ---\n"
                + output[:2400]
            )

        data = json.loads(proc.stdout.strip())
        snapshot = data["snapshot"]
        worktree = snapshot.get("worktree") or {}
        if worktree.get("name") != "feature-x":
            raise RuntimeError(
                "shared worktree snapshot missing name\n--- output ---\n"
                + output[:2400]
            )

        rendered_properties = {
            str(item.get("label")): str(item.get("value"))
            for item in data["properties"]
            if isinstance(item, dict) and item.get("label") is not None
        }
        required_properties = {
            "Worktree": "feature-x · worktree-feature-x",
            "Worktree path": "/tmp/original-repo/.mossen/worktrees/feature-x",
            "Original cwd": "/tmp/original-repo",
            "Original branch": "main",
        }
        missing_properties = [
            f"{label}={expected}"
            for label, expected in required_properties.items()
            if rendered_properties.get(label) != expected
        ]
        if missing_properties:
            raise RuntimeError(
                "worktree provider properties diverged from shared snapshot: "
                + ", ".join(missing_properties)
                + "\n--- output ---\n"
                + output[:2400]
            )

        statusline_worktree = data["statusline"].get("worktree") or {}
        expected_statusline = {
            "name": "feature-x",
            "path": "/tmp/original-repo/.mossen/worktrees/feature-x",
            "branch": "worktree-feature-x",
            "original_cwd": "/tmp/original-repo",
            "original_branch": "main",
        }
        missing_statusline = [
            f"{key}={expected}"
            for key, expected in expected_statusline.items()
            if statusline_worktree.get(key) != expected
        ]
        if missing_statusline:
            raise RuntimeError(
                "statusline worktree snapshot diverged: "
                + ", ".join(missing_statusline)
                + "\n--- output ---\n"
                + output[:2400]
            )

        context_items = {
            str(item["label"]): str(item["value"]) for item in data["contextItems"]
        }
        required_context = {
            "Worktree": "feature-x · worktree-feature-x",
            "Original cwd": "/tmp/original-repo",
            "Original branch": "main",
        }
        missing_context = [
            f"{label}={expected}"
            for label, expected in required_context.items()
            if context_items.get(label) != expected
        ]
        if missing_context:
            raise RuntimeError(
                "context worktree observability diverged: "
                + ", ".join(missing_context)
                + "\n--- output ---\n"
                + output[:2400]
            )

        return {
            "status": "ok",
            "snapshot": snapshot,
            "statuslineWorktree": statusline_worktree,
            "contextItems": context_items,
            "properties": rendered_properties,
        }


def run_resume_worktree_selector_consistency_smoke() -> dict[str, object]:
    probe_script = textwrap.dedent(
        """\
        import {
          getWorktreeMetadataSuffix,
          isLogInCurrentWorktree,
          prioritizeCurrentWorktreeLogs,
        } from './utils/worktreeResume.ts'

        const currentCwd = '/private/tmp/repo/.mossen/worktrees/feature-x'
        const logs = [
          {
            sessionId: 'other-1',
            projectPath: '/tmp/repo/.mossen/worktrees/feature-y',
            worktreeSession: { worktreeName: 'feature-y' },
          },
          {
            sessionId: 'current-1',
            projectPath: currentCwd,
            worktreeSession: { worktreeName: 'feature-x' },
          },
          {
            sessionId: 'other-2',
            projectPath: '/tmp/repo',
          },
          {
            sessionId: 'current-2',
            projectPath: currentCwd,
            worktreeSession: { worktreeName: 'feature-x' },
          },
        ]

        const prioritized = prioritizeCurrentWorktreeLogs(logs, currentCwd)
        console.log(
          JSON.stringify({
            currentFlags: {
              current: isLogInCurrentWorktree(logs[1], currentCwd),
              other: isLogInCurrentWorktree(logs[0], currentCwd),
            },
            prioritized: prioritized.map(log => log.sessionId),
            suffixes: {
              current: getWorktreeMetadataSuffix(logs[1], currentCwd),
              namedOther: getWorktreeMetadataSuffix(logs[0], currentCwd),
              pathOther: getWorktreeMetadataSuffix(logs[2], currentCwd),
            },
          })
        )
        """
    )

    proc = run_cli_capture(
        [RUN_BUN, "-e", probe_script],
        ROOT,
        timeout=120,
    )
    output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
    if proc.returncode != 0:
        raise RuntimeError(
            "resume worktree selector consistency failed\n--- output ---\n"
            + output[:2400]
        )

    data = json.loads(proc.stdout.strip())
    if data["currentFlags"] != {"current": True, "other": False}:
        raise RuntimeError(
            "resume worktree selector current-worktree flags diverged\n--- output ---\n"
            + output[:2400]
        )

    expected_prioritized = ["current-1", "current-2", "other-1", "other-2"]
    if data["prioritized"] != expected_prioritized:
        raise RuntimeError(
            "resume worktree selector prioritization diverged: "
            + repr(data["prioritized"])
            + "\n--- output ---\n"
            + output[:2400]
        )

    current_suffix = data["suffixes"]["current"]
    if current_suffix not in {" · current worktree", " · 当前工作树"}:
        raise RuntimeError(
            "resume worktree selector current suffix diverged: "
            + repr(current_suffix)
            + "\n--- output ---\n"
            + output[:2400]
        )

    expected_suffixes = {
        "namedOther": " · feature-y",
        "pathOther": " · /tmp/repo",
    }
    mismatched = {
        key: expected
        for key, expected in expected_suffixes.items()
        if data["suffixes"].get(key) != expected
    }
    if mismatched:
        raise RuntimeError(
            "resume worktree selector metadata suffixes diverged: "
            + repr(mismatched)
            + "\n--- output ---\n"
            + output[:2400]
        )

    return {
        "status": "ok",
        "prioritized": data["prioritized"],
        "suffixes": data["suffixes"],
    }


def run_resume_title_current_worktree_preference_smoke() -> dict[str, object]:
    probe_script = textwrap.dedent(
        """\
        import {
          selectPreferredCurrentWorktreeLog,
        } from './utils/worktreeResume.ts'

        const currentCwd = '/private/tmp/repo/.mossen/worktrees/feature-x'
        const duplicateTitleLogs = [
          {
            sessionId: 'other-1',
            projectPath: '/tmp/repo/.mossen/worktrees/feature-y',
            customTitle: 'same title',
          },
          {
            sessionId: 'current-1',
            projectPath: currentCwd,
            customTitle: 'same title',
          },
        ]
        const ambiguousCurrentLogs = [
          {
            sessionId: 'current-1',
            projectPath: currentCwd,
            customTitle: 'same title',
          },
          {
            sessionId: 'current-2',
            projectPath: currentCwd,
            customTitle: 'same title',
          },
        ]
        const singleLog = [
          {
            sessionId: 'single-1',
            projectPath: '/tmp/repo/.mossen/worktrees/feature-y',
            customTitle: 'single title',
          },
        ]

        console.log(JSON.stringify({
          duplicateChoice: selectPreferredCurrentWorktreeLog(duplicateTitleLogs, currentCwd)?.sessionId ?? null,
          ambiguousChoice: selectPreferredCurrentWorktreeLog(ambiguousCurrentLogs, currentCwd)?.sessionId ?? null,
          singleChoice: selectPreferredCurrentWorktreeLog(singleLog, currentCwd)?.sessionId ?? null,
        }))
        """
    )

    proc = run_cli_capture(
        [RUN_BUN, "-e", probe_script],
        ROOT,
        timeout=120,
    )
    output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
    if proc.returncode != 0:
        raise RuntimeError(
            "resume title current worktree preference failed\n--- output ---\n"
            + output[:2400]
        )

    data = json.loads(proc.stdout.strip())
    expected = {
        "duplicateChoice": "current-1",
        "ambiguousChoice": None,
        "singleChoice": "single-1",
    }
    if data != expected:
        raise RuntimeError(
            "resume title current worktree preference diverged: "
            + repr(data)
            + "\n--- output ---\n"
            + output[:2400]
        )

    return {
        "status": "ok",
        "choices": data,
    }


def run_resume_startup_worktree_switch_consistency_smoke() -> dict[str, object]:
    probe_script = textwrap.dedent(
        """\
        import { mkdirSync, mkdtempSync } from 'fs'
        import { tmpdir } from 'os'
        import { join } from 'path'
        import { setCwdState, setOriginalCwd } from './bootstrap/state.ts'
        import { exitRestoredWorktree, restoreWorktreeForResume } from './utils/sessionRestore.ts'
        import {
          getCurrentWorktreeObservabilitySnapshot,
          restoreWorktreeSession,
        } from './utils/worktree.ts'

        const root = mkdtempSync(join(tmpdir(), 'mossensrc-resume-worktree-switch-'))
        const original = join(root, 'repo')
        const worktreeA = join(root, 'repo/.mossen/worktrees/feature-a')
        const worktreeB = join(root, 'repo/.mossen/worktrees/feature-b')
        mkdirSync(worktreeA, { recursive: true })
        mkdirSync(worktreeB, { recursive: true })

        const current = {
          originalCwd: original,
          worktreePath: worktreeA,
          worktreeName: 'feature-a',
          worktreeBranch: 'worktree-feature-a',
          originalBranch: 'main',
          sessionId: 'resume-switch-current',
        }
        const target = {
          originalCwd: original,
          worktreePath: worktreeB,
          worktreeName: 'feature-b',
          worktreeBranch: 'worktree-feature-b',
          originalBranch: 'main',
          sessionId: 'resume-switch-target',
        }

        process.chdir(worktreeA)
        setCwdState(worktreeA)
        setOriginalCwd(original)
        restoreWorktreeSession(current)

        exitRestoredWorktree()
        restoreWorktreeForResume(target)

        console.log(JSON.stringify({
          cwd: process.cwd(),
          snapshot: getCurrentWorktreeObservabilitySnapshot(),
        }))
        """
    )

    proc = run_cli_capture(
        [RUN_BUN, "-e", probe_script],
        ROOT,
        timeout=120,
    )
    output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
    if proc.returncode != 0:
        raise RuntimeError(
            "resume startup worktree switch consistency failed\n--- output ---\n"
            + output[:2400]
        )

    data = json.loads(proc.stdout.strip())
    snapshot = data.get("snapshot") or {}
    expected = {
        "name": "feature-b",
        "path_suffix": "/repo/.mossen/worktrees/feature-b",
        "branch": "worktree-feature-b",
        "originalCwd_suffix": "/repo",
        "originalBranch": "main",
    }
    if snapshot.get("name") != expected["name"]:
        raise RuntimeError(
            "resume startup worktree switch snapshot name diverged\n--- output ---\n"
            + output[:2400]
        )
    if not str(data.get("cwd", "")).endswith(expected["path_suffix"]):
        raise RuntimeError(
            "resume startup worktree switch cwd diverged\n--- output ---\n"
            + output[:2400]
        )
    if not str(snapshot.get("path", "")).endswith(expected["path_suffix"]):
        raise RuntimeError(
            "resume startup worktree switch snapshot path diverged\n--- output ---\n"
            + output[:2400]
        )
    if snapshot.get("branch") != expected["branch"]:
        raise RuntimeError(
            "resume startup worktree switch snapshot branch diverged\n--- output ---\n"
            + output[:2400]
        )
    if not str(snapshot.get("originalCwd", "")).endswith(expected["originalCwd_suffix"]):
        raise RuntimeError(
            "resume startup worktree switch original cwd diverged\n--- output ---\n"
            + output[:2400]
        )
    if snapshot.get("originalBranch") != expected["originalBranch"]:
        raise RuntimeError(
            "resume startup worktree switch original branch diverged\n--- output ---\n"
            + output[:2400]
        )

    return {
        "status": "ok",
        "cwd": data["cwd"],
        "snapshot": snapshot,
    }


def run_resume_worktree_session_storage_consistency_smoke() -> dict[str, object]:
    probe_script = textwrap.dedent(
        """\
        import { mkdirSync, writeFileSync, rmSync, realpathSync, utimesSync } from 'fs'
        import { join } from 'path'
        import { setCwdState, setOriginalCwd } from './bootstrap/state.ts'
        import { getProjectDir } from './utils/sessionStoragePortable.ts'
        import { loadSameRepoMessageLogsProgressive } from './utils/sessionStorage.ts'

        process.env.MOSSEN_CONFIG_DIR =
          '/tmp/mossensrc-acceptance-worktree-session-storage-config-' + Math.random().toString(16).slice(2)

        const root = '/tmp/mossensrc-acceptance-worktree-session-storage-' + Math.random().toString(16).slice(2)
        const repo = join(root, 'repo')
        const worktree = join(root, 'repo-wt')
        mkdirSync(repo, { recursive: true })
        mkdirSync(worktree, { recursive: true })

        const repoProjectDir = getProjectDir(repo)
        const worktreeProjectDir = getProjectDir(worktree)
        mkdirSync(repoProjectDir, { recursive: true })
        mkdirSync(worktreeProjectDir, { recursive: true })

        const canonicalRepo = realpathSync(repo)
        const canonicalWorktree = realpathSync(worktree)
        process.chdir(canonicalWorktree)
        setCwdState(canonicalWorktree)
        setOriginalCwd(canonicalWorktree)

        const sharedSessionId = '11111111-1111-4111-8111-111111111111'
        const repoOnlySessionId = '22222222-2222-4222-8222-222222222222'
        const uniqueSessionId = '33333333-3333-4333-8333-333333333333'
        const repoShared = join(repoProjectDir, `${sharedSessionId}.jsonl`)
        const repoOnly = join(repoProjectDir, `${repoOnlySessionId}.jsonl`)
        const worktreeShared = join(worktreeProjectDir, `${sharedSessionId}.jsonl`)
        const worktreeUnique = join(worktreeProjectDir, `${uniqueSessionId}.jsonl`)
        writeFileSync(repoShared, '')
        writeFileSync(repoOnly, '')
        writeFileSync(worktreeShared, '')
        writeFileSync(worktreeUnique, '')
        const tied = new Date('2026-04-22T12:00:00.000Z')
        utimesSync(repoShared, tied, tied)
        utimesSync(worktreeShared, tied, tied)
        const result = await loadSameRepoMessageLogsProgressive([canonicalRepo, canonicalWorktree])

        console.log(JSON.stringify({
          count: result.allStatLogs.length,
          sessions: result.allStatLogs.map(log => ({
            sessionId: log.sessionId,
            projectPath: log.projectPath,
          })),
          sharedProjectPath:
            result.allStatLogs.find(log => log.sessionId === sharedSessionId)?.projectPath ?? null,
        }))

        rmSync(root, { recursive: true, force: true })
        rmSync(process.env.MOSSEN_CONFIG_DIR, { recursive: true, force: true })
        """
    )

    proc = run_cli_capture([RUN_BUN, "-e", probe_script], ROOT, timeout=120)
    output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
    if proc.returncode != 0:
        raise RuntimeError(
            "resume worktree session storage consistency failed\n--- output ---\n"
            + output[:2400]
        )

    data = json.loads(proc.stdout.strip())
    if data.get("count") != 3:
        raise RuntimeError(
            "resume worktree session storage count diverged: "
            + repr(data.get("count"))
            + "\n--- output ---\n"
            + output[:2400]
        )

    sessions = data.get("sessions") or []
    session_ids = {item.get("sessionId") for item in sessions}
    if session_ids != {
        "11111111-1111-4111-8111-111111111111",
        "22222222-2222-4222-8222-222222222222",
        "33333333-3333-4333-8333-333333333333",
    }:
        raise RuntimeError(
            "resume worktree session storage session ids diverged: "
            + repr(session_ids)
            + "\n--- output ---\n"
            + output[:2400]
        )

    project_paths = {str(item.get("projectPath", "")) for item in sessions}
    if not any(path.endswith("/repo") for path in project_paths) or not any(
        path.endswith("/repo-wt") for path in project_paths
    ):
        raise RuntimeError(
            "resume worktree session storage project paths diverged: "
            + repr(project_paths)
            + "\n--- output ---\n"
            + output[:2400]
        )

    shared_project_path = str(data.get("sharedProjectPath", ""))
    if not shared_project_path.endswith("/repo-wt"):
        raise RuntimeError(
            "resume worktree session storage current-worktree dedup diverged: "
            + repr(shared_project_path)
            + "\n--- output ---\n"
            + output[:2400]
        )

    return {
        "status": "ok",
        "count": data["count"],
        "sessions": sessions,
        "sharedProjectPath": data["sharedProjectPath"],
    }


def run_worktree_ide_open_surface_consistency_smoke() -> dict[str, object]:
    ide_text = (ROOT / "commands" / "ide" / "ide.tsx").read_text(errors="ignore")
    probe_script = textwrap.dedent(
        """\
        import {
          getCurrentWorktreeIdeTargetSnapshot,
          restoreWorktreeSession,
        } from './utils/worktree.ts'

        restoreWorktreeSession({
          originalCwd: '/tmp/repo',
          worktreePath: '/tmp/repo/.mossen/worktrees/feature-x',
          worktreeName: 'feature-x',
          worktreeBranch: 'worktree-feature-x',
          originalBranch: 'main',
          sessionId: 'ide-open-worktree',
        })
        const worktree = getCurrentWorktreeIdeTargetSnapshot('/tmp/repo')

        restoreWorktreeSession(null)
        const project = getCurrentWorktreeIdeTargetSnapshot('/tmp/repo')

        console.log(JSON.stringify({ worktree, project }))
        """
    )

    proc = run_cli_capture([RUN_BUN, "-e", probe_script], ROOT, timeout=120)
    output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
    if proc.returncode != 0:
        raise RuntimeError(
            "worktree ide open surface consistency failed\n--- output ---\n"
            + output[:2400]
        )

    data = json.loads(proc.stdout.strip())
    if data.get("worktree") != {
        "kind": "worktree",
        "displayName": "feature-x",
        "path": "/tmp/repo/.mossen/worktrees/feature-x",
        "branch": "worktree-feature-x",
        "originalCwd": "/tmp/repo",
    }:
        raise RuntimeError(
            "worktree ide target snapshot diverged\n--- output ---\n"
            + output[:2400]
        )
    if data.get("project") != {
        "kind": "project",
        "displayName": "repo",
        "path": "/tmp/repo",
        "branch": None,
        "originalCwd": None,
    }:
        raise RuntimeError(
            "project ide target snapshot diverged\n--- output ---\n"
            + output[:2400]
        )

    required_source_markers = [
        "getCurrentWorktreeIdeTargetSnapshot",
        "Select an IDE to open the current worktree",
        "选择用于打开当前工作树的 IDE",
        "Current worktree",
        "当前工作树",
        "Original repo",
        "原始仓库",
        "getLocalizedIdeOpenResult(",
        "getLocalizedIdeOpenManualFallback(",
    ]
    missing = [marker for marker in required_source_markers if marker not in ide_text]
    if missing:
        raise RuntimeError(
            "worktree ide open source surface diverged: " + ", ".join(missing)
        )

    return {
        "status": "ok",
        "worktree": data["worktree"],
        "project": data["project"],
    }


def run_worktree_dev_flow_surface_consistency_smoke() -> dict[str, object]:
    desktop_text = (ROOT / "components" / "DesktopHandoff.tsx").read_text(
        errors="ignore"
    )
    prompt_text = (ROOT / "components" / "ShowInIDEPrompt.tsx").read_text(
        errors="ignore"
    )
    dialog_text = (
        ROOT
        / "components"
        / "permissions"
        / "FilePermissionDialog"
        / "FilePermissionDialog.tsx"
    ).read_text(errors="ignore")
    probe_script = textwrap.dedent(
        """\
        import {
          getCurrentWorktreeDevTargetSnapshot,
          restoreWorktreeSession,
        } from './utils/worktree.ts'

        restoreWorktreeSession({
          originalCwd: '/tmp/repo',
          worktreePath: '/tmp/repo/.mossen/worktrees/feature-x',
          worktreeName: 'feature-x',
          worktreeBranch: 'worktree-feature-x',
          originalBranch: 'main',
          sessionId: 'dev-flow-worktree',
        })
        const worktree = getCurrentWorktreeDevTargetSnapshot('/tmp/repo')

        restoreWorktreeSession(null)
        const project = getCurrentWorktreeDevTargetSnapshot('/tmp/repo')

        console.log(JSON.stringify({ worktree, project }))
        """
    )

    proc = run_cli_capture([RUN_BUN, "-e", probe_script], ROOT, timeout=120)
    output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
    if proc.returncode != 0:
        raise RuntimeError(
            "worktree dev flow surface consistency failed\n--- output ---\n"
            + output[:2400]
        )

    data = json.loads(proc.stdout.strip())
    if data.get("worktree") != {
        "kind": "worktree",
        "displayName": "feature-x",
        "path": "/tmp/repo/.mossen/worktrees/feature-x",
        "branch": "worktree-feature-x",
        "originalCwd": "/tmp/repo",
    }:
        raise RuntimeError(
            "worktree dev target snapshot diverged\n--- output ---\n"
            + output[:2400]
        )
    if data.get("project") != {
        "kind": "project",
        "displayName": "repo",
        "path": "/tmp/repo",
        "branch": None,
        "originalCwd": None,
    }:
        raise RuntimeError(
            "project dev target snapshot diverged\n--- output ---\n"
            + output[:2400]
        )

    desktop_markers = [
        "getCurrentWorktreeDevTargetSnapshot",
        "Current worktree",
        "当前工作树",
        "Worktree path",
        "工作树路径",
        "Original repo",
        "原始仓库",
        "Project path",
        "项目路径",
        "getDesktopHandoffResult(",
    ]
    prompt_markers = [
        "getCurrentWorktreeDevTargetSnapshot",
        "Current worktree",
        "当前工作树",
        "Worktree path",
        "工作树路径",
        "Original repo",
        "原始仓库",
        "Project path",
        "项目路径",
        "outside current worktree",
        "outside current project",
    ]
    dialog_markers = [
        "getCurrentWorktreeDevTargetSnapshot(getCwd())",
        "outside current worktree",
        "outside current project",
        "Symlink target:",
        "符号链接目标：",
    ]
    missing_desktop = [marker for marker in desktop_markers if marker not in desktop_text]
    missing_prompt = [marker for marker in prompt_markers if marker not in prompt_text]
    missing_dialog = [marker for marker in dialog_markers if marker not in dialog_text]
    if missing_desktop or missing_prompt or missing_dialog:
        missing_parts = []
        if missing_desktop:
            missing_parts.append("desktop=" + ", ".join(missing_desktop))
        if missing_prompt:
            missing_parts.append("prompt=" + ", ".join(missing_prompt))
        if missing_dialog:
            missing_parts.append("dialog=" + ", ".join(missing_dialog))
        raise RuntimeError(
            "worktree dev flow source surface diverged: " + " | ".join(missing_parts)
        )

    return {
        "status": "ok",
        "worktree": data["worktree"],
        "project": data["project"],
    }


def _create_worktree_task_fixture(repo: Path, git_env: dict[str, str]) -> None:
    (repo / "README.md").write_text("worktree acceptance fixture\n")
    for args in (
        ["git", "init", "-q", "-b", "main"],
        ["git", "config", "user.name", "Acceptance Bot"],
        ["git", "config", "user.email", "acceptance@example.com"],
        ["git", "add", "."],
        ["git", "commit", "-q", "-m", "fixture"],
    ):
        proc = run_cli_capture(args, repo, timeout=60, env_overrides=git_env)
        if proc.returncode != 0:
            raise RuntimeError(
                f"worktree task fixture setup failed for {' '.join(args)}\n"
                f"--- stdout ---\n{proc.stdout[:1200]}\n--- stderr ---\n{proc.stderr[:1200]}"
            )


def _run_single_worktree_task(
    *,
    repo: Path,
    task_env: dict[str, str],
    worktree_name: str,
    note_name: str,
    note_content: str,
) -> dict[str, object]:
    prompt = (
        "You are running inside a git worktree created for this session.\n"
        "Requirements:\n"
        "- Use Bash to run `pwd`.\n"
        "- Use Bash to run `git branch --show-current`.\n"
        f"- Create a file named `{note_name}` containing exactly `{note_content}`.\n"
        "- Use Bash to run `git status --short` after creating the file.\n"
        f"- Do not create or modify `{note_name}` in the original repository root.\n"
        "- End your final response with a line that starts with `Worktree summary:`."
    )
    proc = run_cli_capture(
        [
            CLI,
            "--worktree",
            worktree_name,
            "--dangerously-skip-permissions",
            "-p",
            "--verbose",
            "--output-format",
            "stream-json",
            "--max-turns",
            "10",
            prompt,
        ],
        repo,
        timeout=420,
        env_overrides=task_env,
    )
    combined = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
    if proc.returncode != 0:
        raise RuntimeError(
            f"worktree task {worktree_name!r} failed with code {proc.returncode}\n--- output ---\n{combined[:4000]}"
        )

    events = parse_stream_json_events(proc.stdout or "")
    tool_blocks = collect_tool_use_blocks(events)
    bash_blocks = [block for block in tool_blocks if block.get("name") == "Bash"]
    saw_pwd = False
    saw_branch = False
    saw_status = False
    assistant_texts: list[str] = []
    for block in bash_blocks:
        tool_input = block.get("input")
        if not isinstance(tool_input, dict):
            continue
        command = tool_input.get("command")
        if not isinstance(command, str):
            continue
        if "pwd" in command:
            saw_pwd = True
        if "git branch --show-current" in command:
            saw_branch = True
        if "git status --short" in command:
            saw_status = True
    for event in events:
        if event.get("type") != "assistant":
            continue
        message = event.get("message")
        if not isinstance(message, dict):
            continue
        content = message.get("content")
        if not isinstance(content, list):
            continue
        for block in content:
            if (
                isinstance(block, dict)
                and block.get("type") == "text"
                and isinstance(block.get("text"), str)
            ):
                assistant_texts.append(block["text"])

    slug = worktree_name.replace("/", "+")
    expected_branch = f"worktree-{slug}"
    worktree_path = repo / ".mossen" / "worktrees" / slug
    note_path = worktree_path / note_name
    repo_note_path = repo / note_name
    if not worktree_path.is_dir():
        raise RuntimeError(
            f"worktree task did not leave the expected worktree directory: {worktree_path}"
        )
    if not note_path.is_file():
        raise RuntimeError(
            f"worktree task did not create {note_name} in the worktree\n--- output ---\n{combined[:4000]}"
        )
    if note_path.read_text().strip() != note_content:
        raise RuntimeError(
            f"worktree task wrote unexpected note content for {note_name}: {note_path.read_text()!r}"
        )
    if repo_note_path.exists():
        raise RuntimeError(
            f"worktree task polluted the original repo root with {repo_note_path}"
        )

    branch_proc = run_cli_capture(
        ["git", "branch", "--show-current"],
        worktree_path,
        timeout=60,
        env_overrides=task_env,
    )
    branch_output = smoke.normalize_output(
        (branch_proc.stdout or "") + (branch_proc.stderr or "")
    ).strip()
    if branch_proc.returncode != 0 or branch_output != expected_branch:
        raise RuntimeError(
            f"worktree task left an unexpected worktree branch\n--- output ---\n{branch_output[:1200]}"
        )

    if not saw_pwd or not saw_branch or not saw_status:
        raise RuntimeError(
            "worktree task stream-json did not record the required Bash workflow\n"
            + json.dumps(
                {
                    "saw_pwd": saw_pwd,
                    "saw_branch": saw_branch,
                    "saw_status": saw_status,
                },
                ensure_ascii=False,
                indent=2,
            )
            + "\n--- output ---\n"
            + combined[:4000]
        )

    assistant_summary = "\n".join(assistant_texts)
    if "Worktree summary:" not in assistant_summary:
        raise RuntimeError(
            f"worktree task final response missing Worktree summary line\n--- output ---\n{combined[:4000]}"
        )

    return {
        "worktree_path": str(worktree_path),
        "note_path": str(note_path),
        "branch": branch_output,
        "assistant_summary_tail": assistant_summary[-400:],
        "bash_calls": len(bash_blocks),
    }


def _collect_repo_status_lines(repo: Path, task_env: dict[str, str]) -> list[str]:
    repo_status_proc = run_cli_capture(
        ["git", "status", "--short"],
        repo,
        timeout=60,
        env_overrides=task_env,
    )
    repo_status_output = smoke.normalize_output(
        (repo_status_proc.stdout or "") + (repo_status_proc.stderr or "")
    )
    repo_status_lines = [
        line.strip()
        for line in repo_status_output.splitlines()
        if line.strip()
    ]
    unexpected_repo_status = [
        line for line in repo_status_lines if line != "?? .mossen/"
    ]
    if repo_status_proc.returncode != 0 or unexpected_repo_status:
        raise RuntimeError(
            "worktree task left the original repo dirty\n--- output ---\n"
            + repo_status_output[:1200]
        )
    return repo_status_lines


def run_worktree_task_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-worktree-task."
    ) as repo_dir, tempfile.TemporaryDirectory(
        prefix="mossensrc-worktree-task-config."
    ) as config_dir:
        repo = Path(repo_dir)
        config = Path(config_dir)

        git_env = {
            "GIT_AUTHOR_NAME": "Acceptance Bot",
            "GIT_AUTHOR_EMAIL": "acceptance@example.com",
            "GIT_COMMITTER_NAME": "Acceptance Bot",
            "GIT_COMMITTER_EMAIL": "acceptance@example.com",
        }
        _create_worktree_task_fixture(repo, git_env)

        task_env = mossen_config_env(config, git_env)
        task_result = _run_single_worktree_task(
            repo=repo,
            task_env=task_env,
            worktree_name="acceptance/demo",
            note_name="worktree-note.txt",
            note_content="worktree acceptance complete",
        )
        repo_status_lines = _collect_repo_status_lines(repo, task_env)

        return {
            "status": "ok",
            **task_result,
            "repo_status_lines": repo_status_lines,
        }


def run_multi_worktree_task_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-multi-worktree-task."
    ) as repo_dir, tempfile.TemporaryDirectory(
        prefix="mossensrc-multi-worktree-task-config."
    ) as config_dir:
        repo = Path(repo_dir)
        config = Path(config_dir)
        seed_temp_config_settings(config)

        git_env = {
            "GIT_AUTHOR_NAME": "Acceptance Bot",
            "GIT_AUTHOR_EMAIL": "acceptance@example.com",
            "GIT_COMMITTER_NAME": "Acceptance Bot",
            "GIT_COMMITTER_EMAIL": "acceptance@example.com",
        }
        _create_worktree_task_fixture(repo, git_env)
        task_env = mossen_config_env(config, git_env)

        alpha = _run_single_worktree_task(
            repo=repo,
            task_env=task_env,
            worktree_name="acceptance/alpha",
            note_name="alpha-note.txt",
            note_content="alpha worktree acceptance complete",
        )
        beta = _run_single_worktree_task(
            repo=repo,
            task_env=task_env,
            worktree_name="acceptance/beta",
            note_name="beta-note.txt",
            note_content="beta worktree acceptance complete",
        )
        alpha_note = Path(str(alpha["note_path"]))
        beta_note = Path(str(beta["note_path"]))
        if not alpha_note.is_file() or not beta_note.is_file():
            raise RuntimeError("multi-worktree task did not leave both worktree note files")
        if (alpha_note.parent / beta_note.name).exists():
            raise RuntimeError("beta note leaked into alpha worktree")
        if (beta_note.parent / alpha_note.name).exists():
            raise RuntimeError("alpha note leaked into beta worktree")

        worktree_list_proc = run_cli_capture(
            ["git", "worktree", "list", "--porcelain"],
            repo,
            timeout=60,
            env_overrides=task_env,
        )
        worktree_list_output = smoke.normalize_output(
            (worktree_list_proc.stdout or "") + (worktree_list_proc.stderr or "")
        )
        if worktree_list_proc.returncode != 0:
            raise RuntimeError(
                "multi-worktree task could not read git worktree list\n--- output ---\n"
                + worktree_list_output[:1200]
            )
        required_worktree_entries = [
            str(repo / ".mossen" / "worktrees" / "acceptance+alpha"),
            str(repo / ".mossen" / "worktrees" / "acceptance+beta"),
            "branch refs/heads/worktree-acceptance+alpha",
            "branch refs/heads/worktree-acceptance+beta",
        ]
        missing_entries = [
            marker for marker in required_worktree_entries if marker not in worktree_list_output
        ]
        if missing_entries:
            raise RuntimeError(
                "multi-worktree task left an unexpected git worktree list\nmissing="
                + json.dumps(missing_entries, ensure_ascii=False)
                + "\n--- output ---\n"
                + worktree_list_output[:1600]
            )

        resume_probe = subprocess.run(
            [
                "bun",
                "-e",
                textwrap.dedent(
                    """
                    import { mkdirSync, realpathSync, utimesSync, writeFileSync } from 'fs'
                    import { join } from 'path'
                    import { enableConfigs } from './utils/config.ts'
                    import { getProjectDir } from './utils/sessionStoragePortable.ts'
                    import { loadSameRepoMessageLogsProgressive } from './utils/sessionStorage.ts'
                    import {
                      prioritizeCurrentWorktreeLogs,
                      selectPreferredCurrentWorktreeLog,
                    } from './utils/worktreeResume.ts'

                    enableConfigs()
                    const repo = process.env.REPO
                    const alphaWorktree = process.env.ALPHA_WORKTREE
                    const betaWorktree = process.env.BETA_WORKTREE
                    if (!repo || !alphaWorktree || !betaWorktree) {
                      throw new Error('missing repo/worktree env for multi-worktree resume probe')
                    }
                    const canonicalRepo = realpathSync(repo)
                    const canonicalAlpha = realpathSync(alphaWorktree)
                    const canonicalBeta = realpathSync(betaWorktree)
                    const alphaProjectDir = getProjectDir(canonicalAlpha)
                    const betaProjectDir = getProjectDir(canonicalBeta)
                    mkdirSync(alphaProjectDir, { recursive: true })
                    mkdirSync(betaProjectDir, { recursive: true })
                    const alphaSessionId = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'
                    const betaSessionId = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb'
                    const alphaFile = join(alphaProjectDir, `${alphaSessionId}.jsonl`)
                    const betaFile = join(betaProjectDir, `${betaSessionId}.jsonl`)
                    writeFileSync(alphaFile, '')
                    writeFileSync(betaFile, '')
                    const tied = new Date('2026-04-22T16:00:00.000Z')
                    utimesSync(alphaFile, tied, tied)
                    utimesSync(betaFile, tied, tied)
                    const result = await loadSameRepoMessageLogsProgressive([
                      canonicalRepo,
                      canonicalAlpha,
                      canonicalBeta,
                    ])
                    const resumable = result.allStatLogs.filter(log =>
                      log.sessionId === alphaSessionId || log.sessionId === betaSessionId
                    )
                    const prioritized = prioritizeCurrentWorktreeLogs(
                      resumable,
                      canonicalAlpha,
                    )
                    const preferred = selectPreferredCurrentWorktreeLog(
                      resumable,
                      canonicalAlpha,
                    )
                    console.log(
                      JSON.stringify({
                        count: resumable.length,
                        sessionIds: resumable.map(log => log.sessionId),
                        projectPaths: resumable.map(log => log.projectPath ?? null),
                        prioritizedProjectPaths: prioritized.map(
                          log => log.projectPath ?? null,
                        ),
                        preferredProjectPath: preferred?.projectPath ?? null,
                      }),
                    )
                    """
                ),
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
            timeout=120,
            env={
                **os.environ,
                MOSSEN_CONFIG_ENV_KEY: str(config),
                "REPO": str(repo),
                "ALPHA_WORKTREE": str(repo / ".mossen" / "worktrees" / "acceptance+alpha"),
                "BETA_WORKTREE": str(repo / ".mossen" / "worktrees" / "acceptance+beta"),
            },
            check=True,
        )
        resume_probe_data = json.loads(
            smoke.normalize_output(resume_probe.stdout.strip())
        )
        session_ids = {
            session_id
            for session_id in resume_probe_data.get("sessionIds", [])
            if isinstance(session_id, str)
        }
        if session_ids != {
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        }:
            raise RuntimeError(
                "multi-worktree task resume probe did not load both worktree session ids\n--- output ---\n"
                + json.dumps(resume_probe_data, ensure_ascii=False, indent=2)
            )
        project_paths = [
            path for path in resume_probe_data.get("projectPaths", []) if isinstance(path, str)
        ]
        if not any("acceptance+alpha" in path for path in project_paths) or not any(
            "acceptance+beta" in path for path in project_paths
        ):
            raise RuntimeError(
                "multi-worktree task resume probe did not load both worktree project paths\n--- output ---\n"
                + json.dumps(resume_probe_data, ensure_ascii=False, indent=2)
            )
        prioritized_paths = [
            path
            for path in resume_probe_data.get("prioritizedProjectPaths", [])
            if isinstance(path, str)
        ]
        if (
            not prioritized_paths
            or "acceptance+alpha" not in prioritized_paths[0]
        ):
            raise RuntimeError(
                "multi-worktree task resume probe did not prioritize the current worktree first\n--- output ---\n"
                + json.dumps(resume_probe_data, ensure_ascii=False, indent=2)
            )
        preferred_project_path = resume_probe_data.get("preferredProjectPath")
        if (
            not isinstance(preferred_project_path, str)
            or "acceptance+alpha" not in preferred_project_path
        ):
            raise RuntimeError(
                "multi-worktree task resume probe did not prefer the current worktree match\n--- output ---\n"
                + json.dumps(resume_probe_data, ensure_ascii=False, indent=2)
            )

        repo_status_lines = _collect_repo_status_lines(repo, task_env)

        return {
            "status": "ok",
            "alpha": alpha,
            "beta": beta,
            "repo_status_lines": repo_status_lines,
            "worktree_list_excerpt": worktree_list_output[:800],
            "resume_probe": resume_probe_data,
        }


def run_rename_command_smoke() -> dict[str, object]:
    proc, fd = smoke.spawn_cli()
    try:
        wait_for_cli_prompt(fd, 25)
        smoke.write_line(fd, "/rename acceptance-smoke-name\n")
        _data, matched = wait_for_cli_patterns(
            fd,
            "/rename acceptance-smoke-name",
            ["Session renamed to: acceptance-smoke-name"],
            40,
        )
        return {"status": "ok", "matched": matched}
    finally:
        try:
            os.close(fd)
        except OSError:
            pass
        smoke.terminate_process_tree(proc)


def run_keybindings_command_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-keybindings-config.") as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)
        slash_inventory = load_slash_command_inventory()
        if not any(str(command.get("name")) == "keybindings" for command in slash_inventory):
            return {
                "status": "ok",
                "matched": "keybindings command unavailable (customization disabled)",
            }
        proc, fd = spawn_cli_custom(
            env_overrides=mossen_config_env(config, {"EDITOR": "true"}),
        )
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "/keybindings\n")
            output, matched = wait_for_cli_patterns(
                fd,
                "/keybindings",
                [
                    "Keybinding customization is not enabled.",
                    "Opened ",
                    "Created ",
                ],
                40,
            )
            result: dict[str, object] = {
                "status": "ok",
                "startup": startup,
                "matched": matched,
            }
            keybindings_path = config / "keybindings.json"
            if matched in {"Opened ", "Created "}:
                if not keybindings_path.exists():
                    raise RuntimeError(
                        "/keybindings reported success but keybindings.json was not created\n"
                        "--- output ---\n"
                        + output[:2000]
                    )
                content = keybindings_path.read_text()
                if '"bindings"' not in content:
                    raise RuntimeError(
                        "/keybindings created keybindings.json without bindings template\n"
                        "--- content ---\n"
                        + content[:2000]
                    )
                result["keybindingsPath"] = str(keybindings_path)
            return result
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)


def run_theme_command_persistence_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-theme-config.") as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)
        global_config_path = config / ".mossen.json"
        global_config = json.loads(global_config_path.read_text())
        global_config["theme"] = "dark"
        global_config_path.write_text(
            json.dumps(global_config, ensure_ascii=False, indent=2) + "\n"
        )

        proc, fd = spawn_cli_custom(
            env_overrides=mossen_config_env(config),
        )
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "/theme\n")
            _picker_data, picker_match = wait_for_cli_patterns(
                fd,
                "/theme",
                [
                    "Theme",
                    "主题",
                    "Dark mode",
                    "深色模式",
                    "Light mode",
                    "浅色模式",
                ],
                50,
            )
            time.sleep(0.2)
            os.write(fd, b"2")
            _result_data, result_match = wait_for_cli_patterns(
                fd,
                "theme select light",
                ["Theme set to light"],
                40,
            )

            updated_global_config = json.loads(global_config_path.read_text())
            if updated_global_config.get("theme") != "light":
                raise RuntimeError(
                    "/theme did not persist the selected theme to .mossen.json\n"
                    "--- config ---\n"
                    + global_config_path.read_text()[:2000]
                )
            return {
                "status": "ok",
                "startup": startup,
                "pickerMatched": picker_match,
                "resultMatched": result_match,
                "theme": updated_global_config.get("theme"),
                "globalConfigPath": str(global_config_path),
            }
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)


def run_branch_command_smoke() -> dict[str, object]:
    def _probe() -> dict[str, object]:
        proc, fd = smoke.spawn_cli()
        try:
            wait_for_cli_prompt(fd, 25)
            smoke.write_line(fd, "Reply with exactly: branch seed ok\n")
            _data, prompt_match = wait_for_cli_patterns(
                fd,
                "branch seed prompt",
                ["branch seed ok"],
                90,
            )
            time.sleep(0.5)
            smoke.write_line(fd, "/branch acceptance-branch\n")
            branch_data, branch_match = wait_for_cli_patterns(
                fd,
                "/branch acceptance-branch",
                [
                    ["Branched conversation", "已创建分支会话"],
                    ["You are now in the branch.", "当前已切换到分支会话。"],
                    ["To resume the original:", "恢复原始会话："],
                    ["Resume with: /resume", "使用以下命令恢复：/resume"],
                ],
                50,
            )
            normalized_branch = smoke.normalize_output(branch_data)
            compacted_branch = compact_text(branch_data)
            branch_resume_match = None
            original_resume_match = re.search(
                r"-r([0-9a-fA-F-]{8,})",
                compacted_branch,
            )
            fork_resume_match = re.search(
                r"/resume([0-9a-fA-F-]{8,})",
                compacted_branch,
            )
            if original_resume_match:
                branch_resume_match = {
                    "kind": "original",
                    "sessionId": original_resume_match.group(1),
                }
            elif fork_resume_match:
                branch_resume_match = {
                    "kind": "fork",
                    "sessionId": fork_resume_match.group(1),
                }
            if branch_resume_match is None:
                raise RuntimeError(
                    "/branch did not surface a concrete resume target\n"
                    "--- output ---\n"
                    + normalized_branch[:2400]
                )
            return {
                "status": "ok",
                "seed_match": prompt_match,
                "branch_match": branch_match,
                "resumeTarget": branch_resume_match,
            }
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

    return with_cli_probe_retry(_probe)


def run_color_command_persistence_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-color-config.") as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)
        seed_prompt = "color history seed alpha"

        proc, fd = spawn_cli_custom(
            cwd=ROOT,
            env_overrides=mossen_config_env(config),
        )
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, f"Reply with exactly: {seed_prompt}\n")
            _seed_data, seed_match = wait_for_cli_patterns(
                fd,
                seed_prompt,
                [seed_prompt],
                90,
            )
            time.sleep(0.5)

            smoke.write_line(fd, "/color blue\n")
            _color_data, color_match = wait_for_cli_patterns(
                fd,
                "/color blue",
                ["Session color set to: blue"],
                60,
            )
            time.sleep(0.2)

            smoke.write_line(fd, "/color default\n")
            _reset_data, reset_match = wait_for_cli_patterns(
                fd,
                "/color default",
                ["Session color reset to default"],
                60,
            )
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

        transcripts = sorted(
            config.glob("projects/**/*.jsonl"),
            key=lambda path: path.stat().st_mtime,
        )
        if not transcripts:
            raise RuntimeError("color command did not create a session transcript")
        transcript_path = transcripts[-1]
        agent_colors: list[str] = []
        for line in transcript_path.read_text().splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                payload = json.loads(line)
            except json.JSONDecodeError:
                continue
            if payload.get("type") == "agent-color" and isinstance(
                payload.get("agentColor"), str
            ):
                agent_colors.append(payload["agentColor"])

        if "blue" not in agent_colors or "default" not in agent_colors:
            raise RuntimeError(
                "color command transcript did not persist both color transitions\n"
                "--- transcript ---\n"
                + transcript_path.read_text()[:2400]
            )

        return {
            "status": "ok",
            "startup": startup,
            "seedMatched": seed_match,
            "colorMatched": color_match,
            "resetMatched": reset_match,
            "agentColors": agent_colors,
            "transcript": str(transcript_path),
        }


def run_export_command_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-export-",
        dir=ROOT,
    ) as export_dir, tempfile.TemporaryDirectory(
        prefix="mossensrc-export-config."
    ) as config_dir:
        export_root = Path(export_dir)
        config = Path(config_dir)
        export_rel = export_root.relative_to(ROOT) / "acceptance-export.txt"
        export_file = ROOT / export_rel
        seed_prompt = "export history seed alpha"

        seed_temp_config_settings(config)

        proc, fd = spawn_cli_custom(
            env_overrides=mossen_config_env(config),
        )
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, f"Reply with exactly: {seed_prompt}\n")
            _reply_data, reply_match = wait_for_cli_patterns(
                fd,
                seed_prompt,
                [seed_prompt],
                90,
            )
            # Some providers render the assistant reply and statusline before the
            # prompt glyph returns. Once we have the seeded reply, continue with
            # the export command instead of hard-blocking on a visible `❯`.
            time.sleep(0.5)

            smoke.write_line(fd, f"/export {export_rel}\n")
            export_data, export_match = wait_for_cli_patterns(
                fd,
                f"/export {export_rel}",
                ["Conversation exported to:"],
                40,
            )
            if not export_file.exists():
                raise RuntimeError(
                    "/export reported success but output file was not created\n"
                    "--- output ---\n"
                    + export_data[:2400]
                )
            content = export_file.read_text()
            if seed_prompt not in content:
                raise RuntimeError(
                    "/export output did not contain the seeded conversation turn\n"
                    "--- content ---\n"
                    + content[:2400]
                )
            return {
                "status": "ok",
                "startup": startup,
                "replyMatched": reply_match,
                "exportMatched": export_match,
                "exportFile": str(export_file),
            }
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)


def run_compact_command_smoke() -> dict[str, object]:
    def _probe() -> dict[str, object]:
        proc, fd = smoke.spawn_cli()
        try:
            wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "Reply with exactly: compact seed one\n")
            _data, first_match = wait_for_cli_patterns(
                fd,
                "compact seed prompt one",
                ["compact seed one"],
                90,
            )
            time.sleep(0.5)
            smoke.write_line(fd, "Reply with exactly: compact seed two\n")
            _data, second_match = wait_for_cli_patterns(
                fd,
                "compact seed prompt two",
                ["compact seed two"],
                90,
            )
            time.sleep(0.5)
            smoke.write_line(fd, "/compact\n")
            _data, compact_match = wait_for_cli_patterns(
                fd,
                "/compact",
                [
                    "Compacted",
                    "Not enough messages",
                    "No messages to compact",
                ],
                120,
            )
            if compact_match != "Compacted":
                raise RuntimeError(
                    "compact command did not complete a real compaction\n"
                    f"matched={compact_match}"
                )
            return {
                "status": "ok",
                "seed_matches": [first_match, second_match],
                "compact_match": compact_match,
            }
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

    return with_cli_probe_retry(_probe)


def run_compact_surface_consistency_smoke() -> dict[str, object]:
    def run_compact_then_probe(command: str, command_label: str) -> dict[str, object]:
        proc, fd = smoke.spawn_cli()
        try:
            wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "Reply with exactly: compact consistency one\n")
            _data, first_match = wait_for_cli_patterns(
                fd,
                "compact consistency seed one",
                ["compact consistency one"],
                90,
            )
            time.sleep(0.5)
            smoke.write_line(fd, "Reply with exactly: compact consistency two\n")
            _data, second_match = wait_for_cli_patterns(
                fd,
                "compact consistency seed two",
                ["compact consistency two"],
                90,
            )
            time.sleep(0.5)
            smoke.write_line(fd, "/compact\n")
            _data, compact_match = wait_for_cli_patterns(
                fd,
                "/compact for surface consistency",
                [
                    "Compacted",
                    "Not enough messages",
                    "No messages to compact",
                ],
                120,
            )
            if compact_match != "Compacted":
                raise RuntimeError(
                    "compact surface consistency did not complete a real compaction\n"
                    f"matched={compact_match}"
                )
            time.sleep(0.5)
            smoke.write_line(fd, f"{command}\n")
            data, matched = wait_for_cli_all_patterns(
                fd,
                command_label,
                [
                    "Recent compact:",
                    "messages since last compact",
                ],
                75,
            )
            normalized = smoke.normalize_output(data)
            if "No compact boundary in this session" in normalized:
                raise RuntimeError(
                    f"{command_label} still reported no compact boundary after /compact\n"
                    f"--- output ---\n{normalized[:2400]}"
                )
            return {
                "seedMatches": [first_match, second_match],
                "compactMatch": compact_match,
                "surfaceMatched": matched,
            }
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

    def _probe() -> dict[str, object]:
        context_result = run_compact_then_probe("/context", "/context after compact")
        status_result = run_compact_then_probe("/status", "/status after compact")
        return {
            "status": "ok",
            "context": context_result,
            "statusSurface": status_result,
        }

    return with_cli_probe_retry(_probe)


def run_model_set_command_smoke() -> dict[str, object]:
    proc, fd = smoke.spawn_cli()
    try:
        wait_for_cli_ready(fd, 30)
        smoke.write_line(fd, "/model example-large\n")
        _data, model_match = wait_for_cli_patterns(
            fd,
            "/model example-large",
            [
                "Set model to",
                "Current model:",
            ],
            90,
        )
        return {
            "status": "ok",
            "matched": model_match,
        }
    finally:
        try:
            os.close(fd)
        except OSError:
            pass
        smoke.terminate_process_tree(proc)


def run_model_surface_consistency_smoke() -> dict[str, object]:
    proc, fd = smoke.spawn_cli()
    try:
        startup = wait_for_cli_ready(fd, 30)
        smoke.write_line(fd, "/model example-large\n")
        _set_data, set_match = wait_for_cli_patterns(
            fd,
            "/model example-large",
            [
                "Set model to",
                "Current model:",
            ],
            90,
        )

        smoke.write_line(fd, "/model current\n")
        current_data, current_match = wait_for_cli_patterns(
            fd,
            "/model current",
            ["Current model:"],
            60,
        )
        normalized_current = smoke.normalize_output(current_data).lower()
        if (
            "example-large" not in normalized_current
            and "sample 3.6 plus" not in normalized_current
        ):
            raise RuntimeError(
                "model current did not reflect example-large session override\n"
                "--- output ---\n"
                + current_data[:2400]
            )

        smoke.write_line(fd, "/status\n")
        status_data, _status_match = wait_for_cli_all_patterns(
            fd,
            "/status after /model",
            [
                "Model:",
                "Model tier:",
            ],
            60,
        )
        compact_status = compact_text(status_data).lower()
        status_required = [
            "model:example-large",
            "modeltier:cloud",
        ]
        status_missing = [
            pattern for pattern in status_required if pattern not in compact_status
        ]
        if status_missing:
            raise RuntimeError(
                f"/status after /model missing {status_missing}\n--- output ---\n"
                + status_data[:3200]
            )

        return {
            "status": "ok",
            "startup": startup,
            "setMatched": set_match,
            "currentMatched": current_match,
            "statusMatched": status_required,
        }
    finally:
        try:
            os.close(fd)
        except OSError:
            pass
        smoke.terminate_process_tree(proc)


def list_config_session_files(config_dir: Path) -> list[Path]:
    projects_dir = config_dir / "projects"
    if not projects_dir.exists():
        return []
    return sorted(projects_dir.rglob("*.jsonl"), key=lambda p: p.stat().st_mtime, reverse=True)


def seed_temp_config_settings(
    config_dir: Path, trusted_dirs: Optional[list[Path | str]] = None
) -> None:
    global_config_src = Path.home() / ".mossen.json"
    global_config_dst = config_dir / ".mossen.json"
    if global_config_src.exists():
        global_config_dst.write_text(global_config_src.read_text())
    else:
        global_config_dst.write_text(
            json.dumps(
                {
                    "theme": "dark",
                    "hasCompletedOnboarding": True,
                },
                ensure_ascii=False,
                indent=2,
            )
            + "\n"
        )

    global_config = json.loads(global_config_dst.read_text())
    projects = global_config.setdefault("projects", {})
    resolved_trusted_dirs = [ROOT.resolve(), *((Path(path).resolve()) for path in (trusted_dirs or []))]
    for trusted_dir in resolved_trusted_dirs:
        project_entry = projects.setdefault(str(trusted_dir), {})
        project_entry["hasTrustDialogAccepted"] = True
    global_config_dst.write_text(
        json.dumps(global_config, ensure_ascii=False, indent=2) + "\n"
    )

    settings_src = Path.home() / ".mossen" / "settings.json"
    settings_dst = config_dir / "settings.json"
    if settings_src.exists():
        settings_dst.write_text(settings_src.read_text())
    else:
        settings_dst.write_text("{}\n")


def run_resume_command_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-resume-config.") as config_dir:
        config = Path(config_dir)
        session_name = "acceptance-resume-session"
        seed_prompt = "resume history seed alpha"

        seed_temp_config_settings(config)

        proc, fd = spawn_cli_custom(
            args=["-n", session_name],
            cwd=ROOT,
            env_overrides=mossen_config_env(config),
        )
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, f"Reply with exactly: {seed_prompt}\n")
            _seed_data, seed_match = wait_for_cli_patterns(
                fd,
                seed_prompt,
                [seed_prompt],
                90,
            )
            rename_match = f"Session name preset: {session_name}"
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

        proc, fd = spawn_cli_custom(
            cwd=ROOT,
            env_overrides=mossen_config_env(config),
        )
        try:
            wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "/resume\n")
            deadline = time.time() + 60
            resume_data = ""
            resume_match = None
            while time.time() < deadline:
                remaining = max(0.1, deadline - time.time())
                readable, _, _ = smoke.select.select([fd], [], [], remaining)
                if not readable:
                    continue
                try:
                    chunk = os.read(fd, 4096).decode("utf-8", errors="ignore")
                except OSError:
                    break
                if not chunk:
                    break
                smoke.respond_to_terminal_queries(fd, chunk)
                resume_data += chunk
                compacted = compact_text(resume_data).lower()
                if "nothingtorew" in compacted:
                    raise RuntimeError(
                        "/resume unexpectedly reported an empty conversation list after seeding history\n"
                        "--- output ---\n"
                        + resume_data[:3200]
                    )
                if session_name in compacted and (
                    "typetosearch" in compacted
                    or "ctrl+vtopreview" in compacted
                    or "resumesession" in compacted
                ):
                    resume_match = "non-empty resume selector"
                    break
            if resume_match is None:
                raise RuntimeError(
                    "/resume did not surface the non-empty conversation selector state\n"
                    "--- output ---\n"
                    + resume_data[:3200]
                )
            return {
                "status": "ok",
                "startup": startup,
                "seedMatched": seed_match,
                "renameMatched": rename_match,
                "resumeMatched": resume_match,
                "sessionName": session_name,
            }
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)


def run_rewind_command_smoke() -> dict[str, object]:
    seed_prompt = "rewind history seed alpha"
    def _probe() -> dict[str, object]:
        proc, fd = smoke.spawn_cli()
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, f"Reply with exactly: {seed_prompt}\n")
            _reply_data, reply_match = wait_for_cli_patterns(
                fd,
                seed_prompt,
                [seed_prompt],
                90,
            )
            time.sleep(0.5)

            smoke.write_line(fd, "/rewind\n")
            deadline = time.time() + 60
            rewind_data = ""
            rewind_match = None
            expected_markers = [
                "restorethecodeand/orconversationtothepointbefore",
                "restoreandforktheconversationtothepointbefore",
                "restorethecodeand/orconversation",
                "restoreandforktheconversation",
            ]
            seed_compact = seed_prompt.replace(" ", "")
            while time.time() < deadline:
                remaining = max(0.1, deadline - time.time())
                readable, _, _ = smoke.select.select([fd], [], [], remaining)
                if not readable:
                    continue
                try:
                    chunk = os.read(fd, 4096).decode("utf-8", errors="ignore")
                except OSError:
                    break
                if not chunk:
                    break
                smoke.respond_to_terminal_queries(fd, chunk)
                rewind_data += chunk
                compacted = compact_text(rewind_data).lower()
                if "nothingtorewindtoyet." in compacted:
                    raise RuntimeError(
                        "/rewind still reported no history after a real conversation turn\n"
                        "--- output ---\n"
                        + rewind_data[:3200]
                    )
                has_restore_marker = any(
                    marker in compacted for marker in expected_markers
                )
                has_current_entry = has_current_selector_entry(compacted)
                has_nonempty_entry_hint = (
                    seed_compact in compacted
                    or "replywithexactly:" + seed_compact in compacted
                    or "entertocontinue" in compacted
                )
                if (
                    has_current_entry
                    and has_nonempty_entry_hint
                    and "rewind" in compacted
                    and has_restore_marker
                ):
                    rewind_match = "non-empty rewind selector"
                    break
            if rewind_match is None:
                raise RuntimeError(
                    "/rewind did not surface the non-empty message selector state\n"
                    "--- output ---\n"
                    + rewind_data[:3200]
                )
            return {
                "status": "ok",
                "startup": startup,
                "replyMatched": reply_match,
                "rewindMatched": rewind_match,
                "seedPrompt": seed_prompt,
            }
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

    return with_cli_probe_retry(_probe)


def run_diff_command_smoke() -> dict[str, object]:
    return run_dialog_command_smoke(
        "/diff",
        [
            "Uncommitted changes",
            "Working tree is clean",
            "git diff HEAD",
            "No file changes in this turn",
        ],
        timeout=60,
    )


def run_diff_worktree_consistency_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-diff-proj.") as repo_dir, tempfile.TemporaryDirectory(
        prefix="mossensrc-diff-config."
    ) as config_dir:
        repo = Path(repo_dir)
        config = Path(config_dir)
        seed_temp_config_settings(config)

        (repo / "notes.txt").write_text("diff fixture\n")
        git_env = {
            "GIT_AUTHOR_NAME": "Acceptance Bot",
            "GIT_AUTHOR_EMAIL": "acceptance@example.com",
            "GIT_COMMITTER_NAME": "Acceptance Bot",
            "GIT_COMMITTER_EMAIL": "acceptance@example.com",
        }
        for args in (
            ["git", "init", "-q"],
            ["git", "config", "user.name", "Acceptance Bot"],
            ["git", "config", "user.email", "acceptance@example.com"],
            ["git", "add", "."],
            ["git", "commit", "-q", "-m", "fixture"],
        ):
            proc = run_cli_capture(args, repo, timeout=60, env_overrides=git_env)
            if proc.returncode != 0:
                raise RuntimeError(
                    f"diff worktree fixture setup failed for {' '.join(args)}\n"
                    f"--- stdout ---\n{proc.stdout[:1200]}\n--- stderr ---\n{proc.stderr[:1200]}"
                )

        (repo / "notes.txt").write_text("diff fixture\nreal diff worktree change\n")

        proc, fd = spawn_cli_custom(
            args=[
                "--dangerously-skip-permissions",
            ],
            cwd=repo,
            env_overrides=mossen_config_env(config, git_env),
        )
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(
                fd,
                "Use Bash exactly once to run: pwd -P. Then reply with exactly: diff cwd ok\n",
            )
            _cwd_data, cwd_match = wait_for_cli_patterns(
                fd,
                "bash cwd into diff worktree",
                ["diff cwd ok"],
                120,
            )
            wait_for_cli_prompt(fd, 30)
            smoke.write_line(fd, "/diff\n")
            _data, matched = wait_for_cli_all_patterns(
                fd,
                "/diff worktree consistency",
                [
                    "Uncommitted changes",
                    "notes.txt",
                    "diff --git",
                ],
                60,
            )
            return {
                "status": "ok",
                "startup": startup,
                "cwdMatched": cwd_match,
                "matched": matched,
                "repo": str(repo),
            }
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)


def run_clear_command_smoke() -> dict[str, object]:
    proc, fd = smoke.spawn_cli()
    try:
        wait_for_cli_prompt(fd, 25)
        smoke.write_line(fd, "/clear\n")
        wait_for_cli_prompt(fd, 60)
        smoke.write_line(fd, "Reply with exactly: clear after ok\n")
        _data, matched = wait_for_cli_patterns(
            fd,
            "clear post command prompt",
            ["clear after ok"],
            90,
        )
        wait_for_cli_prompt(fd, 25)
        return {
            "status": "ok",
            "matched": matched,
        }
    finally:
        try:
            os.close(fd)
        except OSError:
            pass
        smoke.terminate_process_tree(proc)


def run_clear_history_reset_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-clear-config.") as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)
        seed_prompt = "clear history seed alpha"

        proc, fd = spawn_cli_custom(
            cwd=ROOT,
            env_overrides=mossen_config_env(config),
        )
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, f"Reply with exactly: {seed_prompt}\n")
            _seed_data, seed_match = wait_for_cli_patterns(
                fd,
                seed_prompt,
                [seed_prompt],
                90,
            )
            smoke.write_line(fd, "/clear\n")
            wait_for_cli_prompt(fd, 60)
            smoke.write_line(fd, "/rewind\n")
            deadline = time.time() + 60
            rewind_data = ""
            rewind_match = None
            while time.time() < deadline:
                remaining = max(0.1, deadline - time.time())
                readable, _, _ = smoke.select.select([fd], [], [], remaining)
                if not readable:
                    continue
                try:
                    chunk = os.read(fd, 4096).decode("utf-8", errors="ignore")
                except OSError:
                    break
                if not chunk:
                    break
                smoke.respond_to_terminal_queries(fd, chunk)
                rewind_data += chunk
                compacted = compact_text(rewind_data).lower()
                if (
                    "rewind" in compacted
                    and "/clear" in compacted
                    and has_current_selector_entry(compacted)
                ):
                    rewind_match = [
                        "Rewind",
                        "/clear",
                        "current-entry",
                    ]
                    break
            if rewind_match is None:
                raise RuntimeError(
                    "Timed out waiting for /rewind after /clear UI missing current selector entry\n"
                    "--- output ---\n"
                    + rewind_data[:3200]
                )
            return {
                "status": "ok",
                "startup": startup,
                "seedMatched": seed_match,
                "rewindMatched": rewind_match,
            }
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)


def run_memory_command_smoke() -> dict[str, object]:
    return run_dialog_command_smoke(
        "/memory",
        [
            "Use memory files to store durable project guidance for Mossen.",
            "使用记忆文件为编码助手保存持久的项目指导。",
            "Memory",
            "记忆",
        ],
    )


def run_permissions_command_smoke() -> dict[str, object]:
    return run_dialog_command_smoke(
        "/permissions",
        [
            "Permissions:",
            "Workspace",
            "Allow",
            "Ask",
            "Deny",
            "权限",
            "工作区",
        ],
    )


def run_plan_command_smoke() -> dict[str, object]:
    return run_slash_ui_probe("/plan")


def run_plan_surface_consistency_smoke() -> dict[str, object]:
    def _probe() -> dict[str, object]:
        with tempfile.TemporaryDirectory(
            prefix="mossensrc-plan-surface-config."
        ) as config_dir:
            config = Path(config_dir)
            seed_temp_config_settings(config)

            proc, fd = spawn_cli_custom(env_overrides=mossen_config_env(config))
            try:
                startup = wait_for_cli_ready(fd, 30)
                smoke.write_line(fd, "/plan\n")
                _data, command_matched = wait_for_cli_patterns(
                    fd,
                    "/plan",
                    list(SAFE_SLASH_UI_PROBE_SPECS["/plan"]["patterns"]),
                    75,
                )
                smoke.write_line(fd, "/status\n")
                _data, status_matched = wait_for_cli_all_patterns(
                    fd,
                    "/status after /plan",
                    [
                        ["Current permission mode", "当前权限模式"],
                        "plan",
                        ["Plan Mode", "规划模式"],
                    ],
                    60,
                )
            finally:
                try:
                    os.close(fd)
                except OSError:
                    pass
                smoke.terminate_process_tree(proc)

            return {
                "status": "ok",
                "startup": startup,
                "commandMatched": command_matched,
                "statusMatched": status_matched,
            }

    return with_cli_probe_retry(_probe)

def run_permissions_surface_consistency_smoke() -> dict[str, object]:
    def _probe() -> dict[str, object]:
        with tempfile.TemporaryDirectory(
            prefix="mossensrc-permissions-surface-config."
        ) as config_dir:
            config = Path(config_dir)
            seed_temp_config_settings(config)

            proc, fd = spawn_cli_custom(env_overrides=mossen_config_env(config))
            try:
                startup = wait_for_cli_ready(fd, 30)
                smoke.write_line(fd, "/plan\n")
                _data, command_matched = wait_for_cli_patterns(
                    fd,
                    "/plan",
                    list(SAFE_SLASH_UI_PROBE_SPECS["/plan"]["patterns"]),
                    75,
                )
                smoke.write_line(fd, "/permissions\n")
                _data, permissions_matched = wait_for_cli_all_patterns(
                    fd,
                    "/permissions after /plan",
                    [
                        ["Permissions:", "权限：", "权限"],
                        "plan",
                        ["Plan Mode", "规划模式"],
                        ["Add a new rule", "添加新规则"],
                    ],
                    60,
                )
            finally:
                try:
                    os.close(fd)
                except OSError:
                    pass
                smoke.terminate_process_tree(proc)

            return {
                "status": "ok",
                "startup": startup,
                "commandMatched": command_matched,
                "permissionsMatched": permissions_matched,
            }

    return with_cli_probe_retry(_probe)


def run_permissions_accept_edits_surface_consistency_smoke() -> dict[str, object]:
    def _probe() -> dict[str, object]:
        with tempfile.TemporaryDirectory(
            prefix="mossensrc-permissions-accept-edits-config."
        ) as config_dir:
            config = Path(config_dir)
            seed_temp_config_settings(config)

            proc, fd = spawn_cli_custom(
                args=["--permission-mode", "acceptEdits"],
                env_overrides=mossen_config_env(config),
            )
            try:
                startup = wait_for_cli_ready(fd, 30)
                smoke.write_line(fd, "/status\n")
                _data, status_matched = wait_for_cli_all_patterns(
                    fd,
                    "/status in acceptEdits mode",
                    [
                        ["Current permission mode", "当前权限模式"],
                        ["acceptEdits", "Accept edits", "接受修改"],
                    ],
                    60,
                )
            finally:
                try:
                    os.close(fd)
                except OSError:
                    pass
                smoke.terminate_process_tree(proc)

            return {
                "status": "ok",
                "startup": startup,
                "statusMatched": status_matched,
            }

    return with_cli_probe_retry(_probe)


def run_permissions_bypass_surface_consistency_smoke() -> dict[str, object]:
    def _probe() -> dict[str, object]:
        with tempfile.TemporaryDirectory(
            prefix="mossensrc-permissions-bypass-config."
        ) as config_dir:
            config = Path(config_dir)
            seed_temp_config_settings(config)
            settings_path = config / "settings.json"
            if settings_path.exists():
                settings = json.loads(settings_path.read_text())
            else:
                settings = {}
            settings["skipDangerousModePermissionPrompt"] = True
            settings_path.write_text(
                json.dumps(settings, ensure_ascii=False, indent=2) + "\n"
            )

            proc, fd = spawn_cli_custom(
                args=["--dangerously-skip-permissions"],
                env_overrides=mossen_config_env(config),
            )
            try:
                startup = wait_for_cli_ready(fd, 30)
                smoke.write_line(fd, "/status\n")
                _data, status_matched = wait_for_cli_all_patterns(
                    fd,
                    "/status in bypassPermissions mode",
                    [
                        ["Current permission mode", "当前权限模式"],
                        [
                            "bypassPermissions",
                            "Bypass Permissions",
                            "Bypass",
                            "跳过权限",
                        ],
                    ],
                    60,
                )
                os.write(fd, b"\x1b")
                wait_for_cli_prompt(fd, 30)
                smoke.write_line(fd, "/permissions\n")
                _data, permissions_matched = wait_for_cli_all_patterns(
                    fd,
                    "/permissions in bypassPermissions mode",
                    [
                        ["Permissions:", "权限：", "权限"],
                        [
                            "bypassPermissions",
                            "Bypass Permissions",
                            "Bypass",
                            "跳过权限",
                        ],
                    ],
                    60,
                )
            finally:
                try:
                    os.close(fd)
                except OSError:
                    pass
                smoke.terminate_process_tree(proc)

            return {
                "status": "ok",
                "startup": startup,
                "statusMatched": status_matched,
                "permissionsMatched": permissions_matched,
            }

    return with_cli_probe_retry(_probe)


def run_permission_shift_tab_runtime_cycle_smoke() -> dict[str, object]:
    def _probe() -> dict[str, object]:
        with tempfile.TemporaryDirectory(
            prefix="mossensrc-permissions-shift-tab-config."
        ) as config_dir:
            config = Path(config_dir)
            seed_temp_config_settings(config)
            settings_path = config / "settings.json"
            settings = json.loads(settings_path.read_text()) if settings_path.exists() else {}
            settings["skipDangerousModePermissionPrompt"] = True
            settings_path.write_text(
                json.dumps(settings, ensure_ascii=False, indent=2) + "\n"
            )

            proc, fd = spawn_cli_custom(
                args=["--allow-dangerously-skip-permissions"],
                env_overrides=mossen_config_env(config),
            )

            def read_status_mode(label: str, variants: list[object]) -> list[str]:
                smoke.write_line(fd, "/status\n")
                _data, matched = wait_for_cli_all_patterns(
                    fd,
                    label,
                    [
                        ["Current permission mode", "当前权限模式"],
                        variants,
                    ],
                    60,
                )
                os.write(fd, b"\x1b")
                wait_for_cli_prompt(fd, 30)
                return matched

            def cycle_mode() -> None:
                os.write(fd, b"\x1b[Z")
                time.sleep(0.6)

            try:
                startup = wait_for_cli_ready(fd, 30)
                initial_matched = read_status_mode(
                    "/status in initial default mode",
                    ["default", "Default", "默认模式"],
                )

                cycle_mode()
                accept_edits_matched = read_status_mode(
                    "/status after shift+tab to acceptEdits",
                    ["acceptEdits", "Accept edits", "接受修改"],
                )

                cycle_mode()
                plan_matched = read_status_mode(
                    "/status after shift+tab to plan",
                    ["plan", "Plan Mode", "规划模式"],
                )

                cycle_mode()
                bypass_matched = read_status_mode(
                    "/status after shift+tab to bypassPermissions",
                    [
                        "bypassPermissions",
                        "Bypass Permissions",
                        "Bypass",
                        "跳过权限",
                    ],
                )

                cycle_mode()
                wrapped_default_matched = read_status_mode(
                    "/status after shift+tab wraps to default",
                    ["default", "Default", "默认模式"],
                )
            finally:
                try:
                    os.close(fd)
                except OSError:
                    pass
                smoke.terminate_process_tree(proc)

            return {
                "status": "ok",
                "startup": startup,
                "matched": {
                    "initial": initial_matched,
                    "acceptEdits": accept_edits_matched,
                    "plan": plan_matched,
                    "bypassPermissions": bypass_matched,
                    "wrappedDefault": wrapped_default_matched,
                },
                "config": str(settings_path),
            }

    return with_cli_probe_retry(_probe)


def run_color_command_smoke() -> dict[str, object]:
    return run_command_result_smoke(
        "/color default",
        [
            "Session color reset to default",
            "Invalid color",
            "Please provide a color.",
        ],
        timeout=50,
    )


def run_cli_capture(
    args: list[str],
    cwd: Path,
    *,
    timeout: int = 120,
    env_overrides: Optional[dict[str, str]] = None,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    if env_overrides:
        env.update(env_overrides)
    return subprocess.run(
        args,
        cwd=cwd,
        text=True,
        capture_output=True,
        timeout=timeout,
        env=env,
    )


def parse_stream_json_events(raw: str) -> list[dict[str, Any]]:
    events: list[dict[str, Any]] = []
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            parsed = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(parsed, dict):
            events.append(parsed)
    return events


def collect_tool_use_blocks(events: list[dict[str, Any]]) -> list[dict[str, Any]]:
    blocks: list[dict[str, Any]] = []
    for event in events:
        if event.get("type") != "assistant":
            continue
        message = event.get("message")
        if not isinstance(message, dict):
            continue
        content = message.get("content")
        if not isinstance(content, list):
            continue
        for block in content:
            if isinstance(block, dict) and block.get("type") == "tool_use":
                blocks.append(block)
    return blocks


def run_profile_behavior_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-profile-config.") as config_dir:
        settings_src = Path.home() / ".mossen" / "settings.json"
        settings: dict[str, Any] = {}
        if settings_src.exists():
            settings = json.loads(settings_src.read_text())
        settings.update(
            {
                "executionProfile": "review",
                "reasoningProfile": "deep",
                "effortLevel": "high",
            }
        )
        settings_dst = Path(config_dir) / "settings.json"
        settings_dst.write_text(json.dumps(settings, ensure_ascii=False, indent=2) + "\n")

        proc = run_cli_capture(
            [CLI, "auth", "status", "--text"],
            ROOT,
            timeout=90,
            env_overrides={MOSSEN_CONFIG_ENV_KEY: config_dir},
        )
        output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
        if proc.returncode != 0:
            raise RuntimeError(
                f"profile behavior auth status failed\n--- output ---\n{output[:1200]}"
            )
        required_patterns = [
            "Execution profile: review",
            "Reasoning profile: deep",
            "Model tier:",
            "Protocol:",
        ]
        missing = [pattern for pattern in required_patterns if pattern not in output]
        if missing:
            raise RuntimeError(
                f"profile behavior auth status missing {missing}\n--- output ---\n{output[:1600]}"
            )
        return {
            "status": "ok",
            "config": str(settings_dst),
            "matched": required_patterns,
        }


def run_profile_command_persistence_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-profile-command-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)

        proc, fd = spawn_cli_custom(env_overrides=mossen_config_env(config))
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "/profile review deep\n")
            _data, matched = wait_for_cli_all_patterns(
                fd,
                "/profile review deep",
                [
                    [
                        "Set execution profile to review",
                        "已将执行配置设置为 review",
                    ],
                    [
                        "Set reasoning profile to deep",
                        "已将推理配置设置为 deep",
                    ],
                    [
                        "Mapped effort level: high",
                        "映射后的 effort 级别：high",
                    ],
                ],
                60,
            )
            time.sleep(0.5)
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

        settings_path = config / "settings.json"
        settings = json.loads(settings_path.read_text())
        expected = {
            "executionProfile": "review",
            "reasoningProfile": "deep",
            "effortLevel": "high",
        }
        mismatches = {
            key: {"expected": value, "actual": settings.get(key)}
            for key, value in expected.items()
            if settings.get(key) != value
        }
        if mismatches:
            raise RuntimeError(
                "profile command did not persist expected settings\n--- mismatches ---\n"
                + json.dumps(mismatches, ensure_ascii=False, indent=2)
            )
        return {
            "status": "ok",
            "matched": matched,
            "startup": startup,
            "config": str(settings_path),
        }


def run_profile_surface_consistency_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-profile-surface-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)

        proc, fd = spawn_cli_custom(env_overrides=mossen_config_env(config))
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "/profile low-cost fast\n")
            _data, matched = wait_for_cli_all_patterns(
                fd,
                "/profile low-cost fast",
                [
                    [
                        "Set execution profile to low-cost",
                        "已将执行配置设置为 low-cost",
                    ],
                    [
                        "Set reasoning profile to fast",
                        "已将推理配置设置为 fast",
                    ],
                    [
                        "Mapped effort level: low",
                        "映射后的 effort 级别：low",
                    ],
                ],
                60,
            )
            time.sleep(0.5)
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

        settings_path = config / "settings.json"
        settings = json.loads(settings_path.read_text())
        expected = {
            "executionProfile": "low-cost",
            "reasoningProfile": "fast",
            "effortLevel": "low",
        }
        mismatches = {
            key: {"expected": value, "actual": settings.get(key)}
            for key, value in expected.items()
            if settings.get(key) != value
        }
        if mismatches:
            raise RuntimeError(
                "profile surface consistency settings drifted\n--- mismatches ---\n"
                + json.dumps(mismatches, ensure_ascii=False, indent=2)
            )

        auth_proc = run_cli_capture(
            [CLI, "auth", "status", "--text"],
            ROOT,
            timeout=90,
            env_overrides=mossen_config_env(config),
        )
        auth_output = smoke.normalize_output((auth_proc.stdout or "") + (auth_proc.stderr or ""))
        if auth_proc.returncode != 0:
            raise RuntimeError(
                f"profile surface consistency auth status failed\n--- output ---\n{auth_output[:1600]}"
            )
        auth_required = [
            "Execution profile: low-cost",
            "Reasoning profile: fast",
            "Model tier: cloud",
        ]
        auth_missing = [pattern for pattern in auth_required if pattern not in auth_output]
        if auth_missing:
            raise RuntimeError(
                f"profile surface consistency auth status missing {auth_missing}\n--- output ---\n{auth_output[:2000]}"
            )

        proc, fd = spawn_cli_custom(env_overrides=mossen_config_env(config))
        status_patterns = [
            ["Execution", "执行"],
            "low-cost",
            ["profile:", "配置："],
            ["Reasoning", "推理"],
            "fast",
            ["Model tier", "模型层级"],
            ["cloud", "云端"],
        ]
        try:
            wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "/status\n")
            _data, status_matched = wait_for_cli_all_patterns(
                fd,
                "/status profile surface consistency",
                status_patterns,
                60,
            )
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

        return {
            "status": "ok",
            "startup": startup,
            "commandMatched": matched,
            "authMatched": auth_required,
            "statusMatched": status_matched,
            "config": str(settings_path),
        }


def run_effort_command_persistence_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-effort-command-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)

        proc, fd = spawn_cli_custom(env_overrides=mossen_config_env(config))
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "/effort high\n")
            _data, matched = wait_for_cli_patterns(
                fd,
                "/effort high",
                ["Set effort level to high", "Comprehensive implementation"],
                60,
            )
            time.sleep(0.5)
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

        settings_path = config / "settings.json"
        settings = json.loads(settings_path.read_text())
        if settings.get("effortLevel") != "high":
            raise RuntimeError(
                "effort command did not persist high effort level\n--- settings ---\n"
                + json.dumps(settings, ensure_ascii=False, indent=2)
            )

        auth_proc = run_cli_capture(
            [CLI, "auth", "status", "--text"],
            ROOT,
            timeout=90,
            env_overrides=mossen_config_env(config),
        )
        auth_output = smoke.normalize_output((auth_proc.stdout or "") + (auth_proc.stderr or ""))
        if auth_proc.returncode != 0:
            raise RuntimeError(
                f"effort command auth status failed\n--- output ---\n{auth_output[:1200]}"
            )

        required = [
            "Reasoning profile: deep",
            "Execution profile: coding",
        ]
        missing = [pattern for pattern in required if pattern not in auth_output]
        if missing:
            raise RuntimeError(
                f"effort command auth status missing {missing}\n--- output ---\n{auth_output[:1600]}"
            )

        return {
            "status": "ok",
            "matched": matched,
            "startup": startup,
            "config": str(settings_path),
            "authMatched": required,
        }


def get_local_tier_env_overrides() -> dict[str, str]:
    return {
        "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
        "MOSSEN_CODE_CUSTOM_BASE_URL": "http://127.0.0.1:9797/v1",
        "MOSSEN_CODE_CUSTOM_NAME": "Local Gemma4",
        "MOSSEN_CODE_CUSTOM_MODEL": "gemma4",
        "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
        "MOSSEN_CODE_CUSTOM_API_KEY": "local-test-key",
    }


def run_model_tier_local_surface_smoke() -> dict[str, object]:
    env_overrides = get_local_tier_env_overrides()

    auth_proc = run_cli_capture(
        [CLI, "auth", "status", "--text"],
        ROOT,
        timeout=90,
        env_overrides=env_overrides,
    )
    auth_output = smoke.normalize_output((auth_proc.stdout or "") + (auth_proc.stderr or ""))
    if auth_proc.returncode != 0:
        raise RuntimeError(
            f"local tier auth status failed\n--- output ---\n{auth_output[:1600]}"
        )

    auth_required = [
        "Login method: Local Gemma4",
        "Backend URL: http://127.0.0.1:9797/v1",
        "Custom model: gemma4",
        "Context window: 200,000 tokens",
        "Model tier: local",
        "Protocol: openai-compatible",
    ]
    auth_missing = [pattern for pattern in auth_required if pattern not in auth_output]
    if auth_missing:
        raise RuntimeError(
            f"local tier auth status missing {auth_missing}\n--- output ---\n{auth_output[:2000]}"
        )

    platform_proc = run_cli_capture(
        [RUN_BUN, "scripts/platform_check.ts"],
        ROOT,
        timeout=120,
        env_overrides=env_overrides,
    )
    platform_output = smoke.normalize_output((platform_proc.stdout or "") + (platform_proc.stderr or ""))
    if platform_proc.returncode != 0:
        raise RuntimeError(
            f"local tier platform_check failed\n--- output ---\n{platform_output[:2000]}"
        )

    try:
        payload = json.loads(platform_output)
    except json.JSONDecodeError as exc:
        raise RuntimeError(
            f"local tier platform_check returned unexpected payload: {exc}\n--- output ---\n{platform_output[:2000]}"
        ) from exc

    provider = next(
        (check.get("detail") for check in payload.get("checks", []) if check.get("name") == "provider"),
        None,
    )
    if not isinstance(provider, dict):
        raise RuntimeError(
            "local tier platform_check missing provider detail\n--- output ---\n"
            + platform_output[:2000]
        )

    expected_provider = {
        "tier": "local",
        "protocol": "openai-compatible",
        "baseUrl": "http://127.0.0.1:9797/v1",
        "model": "gemma4",
    }
    mismatches = {
        key: {"expected": value, "actual": provider.get(key)}
        for key, value in expected_provider.items()
        if provider.get(key) != value
    }
    if mismatches:
        raise RuntimeError(
            "local tier platform provider detail drifted\n--- mismatches ---\n"
            + json.dumps(mismatches, ensure_ascii=False, indent=2)
        )

    return {
        "status": "ok",
        "authMatched": auth_required,
        "platformProvider": expected_provider,
    }


def run_statusline_runtime_surface_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-statusline-surface-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)

        statusline_script = config / "statusline-surface.py"
        statusline_script.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import json
                import sys

                payload = json.load(sys.stdin)
                ctx = (payload.get("context_window") or {}).get("used_percentage")
                if ctx is None:
                    ctx = (payload.get("context_observability") or {}).get("pressure_percent", "")
                print(
                    "tier={tier} profile={profile} reason={reason} compact={compact} ctx={ctx}".format(
                        tier=payload.get("model_tier", ""),
                        profile=(payload.get("profiles") or {}).get("execution", ""),
                        reason=(payload.get("profiles") or {}).get("reasoning", ""),
                        compact=(payload.get("context_observability") or {}).get("recent_compact", ""),
                        ctx=ctx,
                    )
                )
                """
            )
        )
        statusline_script.chmod(0o755)

        settings_path = config / "settings.json"
        settings: dict[str, Any] = {}
        if settings_path.exists():
            settings = json.loads(settings_path.read_text())
        settings.update(
            {
                "statusLine": {
                    "type": "command",
                    "command": str(statusline_script),
                    "padding": 0,
                },
                "executionProfile": "review",
                "reasoningProfile": "deep",
                "effortLevel": "high",
            }
        )
        settings_path.write_text(
            json.dumps(settings, ensure_ascii=False, indent=2) + "\n"
        )

        probe_script = textwrap.dedent(
            """\
            import { setSessionTrustAccepted } from './bootstrap/state.ts'
            import { executeStatusLineCommand } from './utils/hooks.ts'
            import { buildStatusLineObservabilityInput } from './utils/statusLineObservability.ts'

            setSessionTrustAccepted(true)

            const settings = {
              executionProfile: 'review',
              reasoningProfile: 'deep',
              effortLevel: 'high',
            } as const

            const model = 'example-large'
            const observability = buildStatusLineObservabilityInput([], model, 'high', settings, {
              autoCompactEnabled: false,
              modelTier: 'cloud',
            })
            observability.context_observability.pressure_percent = 1

            const payload = {
              session_id: 'personal-acceptance-statusline',
              transcript_path: '/tmp/personal-acceptance-statusline.jsonl',
              cwd: process.cwd(),
              model: {
                id: model,
                display_name: 'Qwen 3.6 Plus',
              },
              workspace: {
                current_dir: process.cwd(),
                project_dir: process.cwd(),
                added_dirs: [],
              },
              version: 'acceptance',
              output_style: {
                name: 'default',
              },
              context_window: {
                total_input_tokens: 0,
                total_output_tokens: 0,
                context_window_size: 200000,
                current_usage: null,
                used_percentage: 18,
                remaining_percentage: 82,
              },
              exceeds_200k_tokens: false,
              ...observability,
            }

            const output = await executeStatusLineCommand(payload)
            if (!output) {
              throw new Error('statusline runtime returned no output')
            }
            process.stdout.write(output + '\\n')
            """
        )

        proc = run_cli_capture(
            [RUN_BUN, "-e", probe_script],
            ROOT,
            timeout=120,
            env_overrides=mossen_config_env(config),
        )
        output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
        if proc.returncode != 0:
            raise RuntimeError(
                f"statusline runtime surface failed\n--- output ---\n{output[:2400]}"
            )

        required = [
            "tier=cloud",
            "profile=review",
            "reason=deep",
            "compact=none",
            "ctx=18",
        ]
        missing = [pattern for pattern in required if pattern not in output]
        if missing:
            raise RuntimeError(
                f"statusline runtime surface missing {missing}\n--- output ---\n{output[:2400]}"
            )

        return {
            "status": "ok",
            "matched": required,
            "script": str(statusline_script),
        }


def run_statusline_runtime_surface_local_tier_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-statusline-local-tier-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)

        statusline_script = config / "statusline-local-tier.py"
        statusline_script.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import json
                import sys

                payload = json.load(sys.stdin)
                print(
                    "tier={tier} profile={profile} reason={reason}".format(
                        tier=payload.get("model_tier", ""),
                        profile=(payload.get("profiles") or {}).get("execution", ""),
                        reason=(payload.get("profiles") or {}).get("reasoning", ""),
                    )
                )
                """
            )
        )
        statusline_script.chmod(0o755)

        settings_path = config / "settings.json"
        settings: dict[str, Any] = {}
        if settings_path.exists():
            settings = json.loads(settings_path.read_text())
        settings.update(
            {
                "statusLine": {
                    "type": "command",
                    "command": str(statusline_script),
                    "padding": 0,
                },
            }
        )
        settings_path.write_text(
            json.dumps(settings, ensure_ascii=False, indent=2) + "\n"
        )

        probe_script = textwrap.dedent(
            """\
            import { setSessionTrustAccepted } from './bootstrap/state.ts'
            import { executeStatusLineCommand } from './utils/hooks.ts'
            import { buildStatusLineObservabilityInput } from './utils/statusLineObservability.ts'

            setSessionTrustAccepted(true)

            const settings = {} as const
            const model = 'gemma4'
            const payload = {
              session_id: 'personal-acceptance-statusline-local-tier',
              transcript_path: '/tmp/personal-acceptance-statusline-local-tier.jsonl',
              cwd: process.cwd(),
              model: {
                id: model,
                display_name: 'Local Gemma4',
              },
              workspace: {
                current_dir: process.cwd(),
                project_dir: process.cwd(),
                added_dirs: [],
              },
              version: 'acceptance',
              output_style: {
                name: 'default',
              },
              context_window: {
                total_input_tokens: 0,
                total_output_tokens: 0,
                context_window_size: 200000,
                current_usage: null,
                used_percentage: 0,
                remaining_percentage: 100,
              },
              exceeds_200k_tokens: false,
              ...buildStatusLineObservabilityInput([], model, undefined, settings, {
                autoCompactEnabled: false,
              }),
            }

            const output = await executeStatusLineCommand(payload)
            if (!output) {
              throw new Error('statusline runtime returned no output')
            }
            process.stdout.write(output + '\\n')
            """
        )

        env_overrides = get_local_tier_env_overrides() | {
            MOSSEN_CONFIG_ENV_KEY: str(config),
        }
        proc = run_cli_capture(
            [RUN_BUN, "-e", probe_script],
            ROOT,
            timeout=120,
            env_overrides=env_overrides,
        )
        output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
        if proc.returncode != 0:
            raise RuntimeError(
                f"statusline local tier surface failed\n--- output ---\n{output[:2400]}"
            )

        required = [
            "tier=local",
            "profile=coding",
            "reason=standard",
        ]
        missing = [pattern for pattern in required if pattern not in output]
        if missing:
            raise RuntimeError(
                f"statusline local tier surface missing {missing}\n--- output ---\n{output[:2400]}"
            )

        return {
            "status": "ok",
            "matched": required,
            "script": str(statusline_script),
        }


def run_statusline_runtime_surface_compact_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-statusline-compact-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)

        statusline_script = config / "statusline-compact.py"
        statusline_script.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import json
                import sys

                payload = json.load(sys.stdin)
                print(
                    "tier={tier} profile={profile} reason={reason} compact={compact}".format(
                        tier=payload.get("model_tier", ""),
                        profile=(payload.get("profiles") or {}).get("execution", ""),
                        reason=(payload.get("profiles") or {}).get("reasoning", ""),
                        compact=(payload.get("context_observability") or {}).get("recent_compact", ""),
                    )
                )
                """
            )
        )
        statusline_script.chmod(0o755)

        settings_path = config / "settings.json"
        settings: dict[str, Any] = {}
        if settings_path.exists():
            settings = json.loads(settings_path.read_text())
        settings.update(
            {
                "statusLine": {
                    "type": "command",
                    "command": str(statusline_script),
                    "padding": 0,
                },
                "executionProfile": "review",
                "reasoningProfile": "deep",
                "effortLevel": "high",
            }
        )
        settings_path.write_text(
            json.dumps(settings, ensure_ascii=False, indent=2) + "\n"
        )

        probe_script = textwrap.dedent(
            """\
            import { setSessionTrustAccepted } from './bootstrap/state.ts'
            import { executeStatusLineCommand } from './utils/hooks.ts'
            import { buildStatusLineObservabilityInput } from './utils/statusLineObservability.ts'

            setSessionTrustAccepted(true)

            const settings = {
              executionProfile: 'review',
              reasoningProfile: 'deep',
              effortLevel: 'high',
            } as const

            const model = 'example-large'
            const messages = [
              {
                type: 'system',
                content: 'Conversation compacted',
                level: 'info',
                subtype: 'compact_boundary',
              },
              {
                type: 'user',
                message: {
                  role: 'user',
                  content: 'after compact alpha',
                },
              },
              {
                type: 'assistant',
                message: {
                  role: 'assistant',
                  content: [
                    {
                      type: 'text',
                      text: 'after compact beta',
                    },
                  ],
                },
              },
            ] as any[]

            const payload = {
              session_id: 'personal-acceptance-statusline-compact',
              transcript_path: '/tmp/personal-acceptance-statusline-compact.jsonl',
              cwd: process.cwd(),
              model: {
                id: model,
                display_name: 'Qwen 3.6 Plus',
              },
              workspace: {
                current_dir: process.cwd(),
                project_dir: process.cwd(),
                added_dirs: [],
              },
              version: 'acceptance',
              output_style: {
                name: 'default',
              },
              context_window: {
                total_input_tokens: 0,
                total_output_tokens: 0,
                context_window_size: 200000,
                current_usage: null,
                used_percentage: 0,
                remaining_percentage: 100,
              },
              exceeds_200k_tokens: false,
              ...buildStatusLineObservabilityInput(messages as any, model, 'high', settings, {
                autoCompactEnabled: true,
                modelTier: 'cloud',
              }),
            }

            const output = await executeStatusLineCommand(payload)
            if (!output) {
              throw new Error('statusline compact runtime returned no output')
            }
            process.stdout.write(output + '\\n')
            """
        )

        proc = run_cli_capture(
            [RUN_BUN, "-e", probe_script],
            ROOT,
            timeout=120,
            env_overrides=mossen_config_env(config),
        )
        output = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
        if proc.returncode != 0:
            raise RuntimeError(
                f"statusline compact runtime surface failed\n--- output ---\n{output[:2400]}"
            )

        required = [
            "tier=cloud",
            "profile=review",
            "reason=deep",
            "compact=2 messages since last compact",
        ]
        missing = [pattern for pattern in required if pattern not in output]
        if missing:
            raise RuntimeError(
                f"statusline compact runtime surface missing {missing}\n--- output ---\n{output[:2400]}"
            )

        return {
            "status": "ok",
            "matched": required,
            "script": str(statusline_script),
        }


def run_config_default_permission_mode_surface_smoke() -> dict[str, object]:
    label_variants = [
        "Default permission mode",
        "Dfault permission mode",
        "默认权限模式",
    ]
    value_patterns = ["Accept edits", "Accpt edits", "接受修改"]
    last_error: Exception | None = None
    for _attempt in range(2):
        try:
            with tempfile.TemporaryDirectory(
                prefix="mossensrc-config-default-permission-config."
            ) as config_dir:
                config = Path(config_dir)
                seed_temp_config_settings(config)
                settings_path = config / "settings.json"
                settings = json.loads(settings_path.read_text())
                settings.setdefault("permissions", {})["defaultMode"] = "acceptEdits"
                settings_path.write_text(
                    json.dumps(settings, ensure_ascii=False, indent=2) + "\n"
                )

                proc, fd = spawn_cli_custom(
                    env_overrides=mossen_config_env(config)
                )
                try:
                    startup = wait_for_cli_ready(fd, 30)
                    smoke.write_line(fd, "/config\n")
                    _data, command_matched = wait_for_cli_patterns(
                        fd,
                        "/config open",
                        ["Config", "配置"],
                        30,
                    )
                    os.write(fd, b"/")
                    _data, search_box_matched = wait_for_cli_patterns(
                        fd,
                        "/config search open",
                        ["Search settings", "搜索设置"],
                        20,
                    )
                    os.write(fd, b"Default permission mode")
                    config_data, config_surface_matched = wait_for_cli_patterns(
                        fd,
                        "/config search default permission mode",
                        [label_variants + value_patterns],
                        60,
                    )
                    normalized_config = smoke.normalize_output(config_data)
                    compact_config = compact_text(config_data)
                    config_label_matched = next(
                        (
                            variant
                            for variant in label_variants
                            if compact_text(variant) in compact_config
                        ),
                        None,
                    )
                    config_value_matched = next(
                        (
                            pattern
                            for pattern in value_patterns
                            if pattern in normalized_config
                            or compact_text(pattern) in compact_config
                        ),
                        None,
                    )
                    if config_value_matched is None:
                        _data, config_value_matched = wait_for_cli_patterns(
                            fd,
                            "/config search default permission mode value",
                            value_patterns,
                            60,
                        )
                    if config_label_matched is None:
                        config_label_matched = config_surface_matched
                finally:
                    try:
                        os.close(fd)
                    except OSError:
                        pass
                    smoke.terminate_process_tree(proc)

                return {
                    "status": "ok",
                    "startup": startup,
                    "commandMatched": command_matched,
                    "searchMatched": search_box_matched,
                    "configMatched": [config_label_matched, config_value_matched],
                    "config": str(settings_path),
                }
        except Exception as exc:
            last_error = exc
            time.sleep(0.2)
    assert last_error is not None
    raise last_error


def run_default_permission_mode_runtime_consistency_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="mossensrc-default-permission-runtime-config."
    ) as config_dir:
        config = Path(config_dir)
        seed_temp_config_settings(config)
        settings_path = config / "settings.json"
        settings = json.loads(settings_path.read_text())
        settings.setdefault("permissions", {})["defaultMode"] = "acceptEdits"
        settings_path.write_text(
            json.dumps(settings, ensure_ascii=False, indent=2) + "\n"
        )

        auth_proc = run_cli_capture(
            [CLI, "auth", "status", "--text"],
            ROOT,
            timeout=90,
            env_overrides=mossen_config_env(config),
        )
        auth_output = smoke.normalize_output((auth_proc.stdout or "") + (auth_proc.stderr or ""))
        if auth_proc.returncode != 0:
            raise RuntimeError(
                f"default permission mode auth status failed\n--- output ---\n{auth_output[:1600]}"
            )
        auth_required = [
            "Execution profile:",
            "Reasoning profile:",
            "permissions=acceptEdits",
        ]
        auth_missing = [pattern for pattern in auth_required if pattern not in auth_output]
        if auth_missing:
            raise RuntimeError(
                f"default permission mode auth status missing {auth_missing}\n--- output ---\n{auth_output[:2000]}"
            )

        proc, fd = spawn_cli_custom(env_overrides=mossen_config_env(config))
        try:
            startup = wait_for_cli_ready(fd, 30)
            smoke.write_line(fd, "/status\n")
            status_data, status_surface_matched = wait_for_cli_patterns(
                fd,
                "/status with default permission mode from settings",
                [["Current permission mode", "当前权限模式", "Accept edits", "接受修改"]],
                60,
            )
            normalized_status = smoke.normalize_output(status_data)
            compact_status = compact_text(status_data)
            status_label_matched = next(
                (
                    pattern
                    for pattern in ["Current permission mode", "当前权限模式"]
                    if pattern in normalized_status or compact_text(pattern) in compact_status
                ),
                None,
            )
            status_value_matched = next(
                (
                    pattern
                    for pattern in ["Accept edits", "接受修改"]
                    if pattern in normalized_status or compact_text(pattern) in compact_status
                ),
                None,
            )
            if status_label_matched is None:
                status_label_matched = status_surface_matched
            if status_value_matched is None:
                _data, status_value_matched = wait_for_cli_patterns(
                    fd,
                    "/status with default permission mode from settings value",
                    ["Accept edits", "接受修改"],
                    60,
                )
        finally:
            try:
                os.close(fd)
            except OSError:
                pass
            smoke.terminate_process_tree(proc)

        return {
            "status": "ok",
            "startup": startup,
            "authMatched": auth_required,
            "statusMatched": [status_label_matched, status_value_matched],
            "config": str(settings_path),
        }




def run_bugfix_task_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-bugfix-task.") as repo_dir, tempfile.TemporaryDirectory(
        prefix="mossensrc-bugfix-config."
    ) as config_dir:
        repo = Path(repo_dir)
        config = Path(config_dir)

        (repo / "calc.js").write_text(
            "function add(a, b) {\n"
            "  return a - b;\n"
            "}\n\n"
            "module.exports = { add };\n"
        )
        (repo / "test.js").write_text(
            "const assert = require('node:assert/strict');\n"
            "const { add } = require('./calc');\n\n"
            "assert.equal(add(2, 3), 5);\n"
            "console.log('bugfix task tests passed');\n"
        )

        git_env = {
            "GIT_AUTHOR_NAME": "Acceptance Bot",
            "GIT_AUTHOR_EMAIL": "acceptance@example.com",
            "GIT_COMMITTER_NAME": "Acceptance Bot",
            "GIT_COMMITTER_EMAIL": "acceptance@example.com",
        }
        for args in (
            ["git", "init", "-q"],
            ["git", "config", "user.name", "Acceptance Bot"],
            ["git", "config", "user.email", "acceptance@example.com"],
            ["git", "add", "."],
            ["git", "commit", "-q", "-m", "fixture"],
        ):
            proc = run_cli_capture(args, repo, timeout=60, env_overrides=git_env)
            if proc.returncode != 0:
                raise RuntimeError(
                    f"bugfix task fixture setup failed for {' '.join(args)}\n"
                    f"--- stdout ---\n{proc.stdout[:1200]}\n--- stderr ---\n{proc.stderr[:1200]}"
                )

        prompt = (
            f"The only repository you may modify is: {repo}\n"
            "Do not modify the current Mossen workspace.\n"
            "Fix the failing test in that repository.\n"
            "Requirements:\n"
            f"- Use Bash to `cd {repo}` and run `node test.js`.\n"
            "- Do not modify test.js.\n"
            "- Edit the implementation until the test passes.\n"
            "- End your final response with a line that starts with `Summary:`."
        )
        task_env = {
            MOSSEN_CONFIG_ENV_KEY: str(config),
            **git_env,
        }
        proc = run_cli_capture(
            [
                CLI,
                "--add-dir",
                str(repo),
                "--dangerously-skip-permissions",
                "-p",
                "--verbose",
                "--output-format",
                "stream-json",
                "--max-turns",
                "10",
                prompt,
            ],
            ROOT,
            timeout=360,
            env_overrides=task_env,
        )
        combined = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
        if proc.returncode != 0:
            raise RuntimeError(
                f"bugfix task failed with code {proc.returncode}\n--- output ---\n{combined[:4000]}"
            )

        events = parse_stream_json_events(proc.stdout or "")
        bash_calls = 0
        assistant_texts: list[str] = []
        for event in events:
            if event.get("type") != "assistant":
                continue
            message = event.get("message")
            if not isinstance(message, dict):
                continue
            content = message.get("content")
            if not isinstance(content, list):
                continue
            for block in content:
                if not isinstance(block, dict):
                    continue
                if block.get("type") == "tool_use" and block.get("name") == "Bash":
                    bash_calls += 1
                if block.get("type") == "text" and isinstance(block.get("text"), str):
                    assistant_texts.append(block["text"])

        test_proc = run_cli_capture(["node", "test.js"], repo, timeout=60, env_overrides=task_env)
        test_output = smoke.normalize_output((test_proc.stdout or "") + (test_proc.stderr or ""))
        if test_proc.returncode != 0 or "bugfix task tests passed" not in test_output:
            raise RuntimeError(
                f"bugfix task did not leave the fixture passing\n--- output ---\n{test_output[:1200]}"
            )

        diff_proc = run_cli_capture(["git", "diff", "--name-only", "HEAD"], repo, timeout=60, env_overrides=task_env)
        diff_output = smoke.normalize_output((diff_proc.stdout or "") + (diff_proc.stderr or ""))
        changed_files = [line.strip() for line in diff_output.splitlines() if line.strip()]
        if diff_proc.returncode != 0 or not changed_files:
            raise RuntimeError(
                f"bugfix task did not leave any edited files\n--- output ---\n{diff_output[:1200]}"
            )
        if "test.js" in changed_files:
            raise RuntimeError(
                f"bugfix task modified test.js, which should stay untouched\nchanged={changed_files}"
            )
        if bash_calls == 0:
            raise RuntimeError(
                f"bugfix task stream-json did not record a Bash tool call\n--- output ---\n{combined[:4000]}"
            )

        assistant_summary = "\n".join(assistant_texts)
        if "Summary:" not in assistant_summary:
            raise RuntimeError(
                f"bugfix task final response missing Summary line\n--- output ---\n{combined[:4000]}"
            )

        return {
            "status": "ok",
            "bash_calls": bash_calls,
            "changed_files": changed_files,
            "assistant_summary_tail": assistant_summary[-400:],
        }


def run_git_flow_task_smoke() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-git-flow-task.") as repo_dir, tempfile.TemporaryDirectory(
        prefix="mossensrc-git-flow-config."
    ) as config_dir:
        repo = Path(repo_dir)
        config = Path(config_dir)

        (repo / "notes.txt").write_text("git flow fixture\n")

        git_env = {
            "GIT_AUTHOR_NAME": "Acceptance Bot",
            "GIT_AUTHOR_EMAIL": "acceptance@example.com",
            "GIT_COMMITTER_NAME": "Acceptance Bot",
            "GIT_COMMITTER_EMAIL": "acceptance@example.com",
        }
        for args in (
            ["git", "init", "-q"],
            ["git", "config", "user.name", "Acceptance Bot"],
            ["git", "config", "user.email", "acceptance@example.com"],
            ["git", "add", "."],
            ["git", "commit", "-q", "-m", "fixture"],
        ):
            proc = run_cli_capture(args, repo, timeout=60, env_overrides=git_env)
            if proc.returncode != 0:
                raise RuntimeError(
                    f"git flow task fixture setup failed for {' '.join(args)}\n"
                    f"--- stdout ---\n{proc.stdout[:1200]}\n--- stderr ---\n{proc.stderr[:1200]}"
                )

        prompt = (
            f"The only repository you may modify is: {repo}\n"
            "Do not modify the current Mossen workspace.\n"
            "Append the exact line `git flow acceptance complete` to notes.txt.\n"
            "Requirements:\n"
            f"- Use Bash to `cd {repo}`.\n"
            "- Inspect `git diff` after making the edit.\n"
            "- Create a local git commit for the change.\n"
            "- End your final response with a line that starts with `Commit summary:`."
        )
        task_env = {
            MOSSEN_CONFIG_ENV_KEY: str(config),
            **git_env,
        }
        proc = run_cli_capture(
            [
                CLI,
                "--add-dir",
                str(repo),
                "--dangerously-skip-permissions",
                "-p",
                "--verbose",
                "--output-format",
                "stream-json",
                "--max-turns",
                "10",
                prompt,
            ],
            ROOT,
            timeout=360,
            env_overrides=task_env,
        )
        combined = smoke.normalize_output((proc.stdout or "") + (proc.stderr or ""))
        if proc.returncode != 0:
            raise RuntimeError(
                f"git flow task failed with code {proc.returncode}\n--- output ---\n{combined[:4000]}"
            )

        events = parse_stream_json_events(proc.stdout or "")
        tool_blocks = collect_tool_use_blocks(events)
        bash_blocks = [block for block in tool_blocks if block.get("name") == "Bash"]
        saw_git_diff = False
        for block in bash_blocks:
            tool_input = block.get("input")
            if not isinstance(tool_input, dict):
                continue
            command = tool_input.get("command")
            if isinstance(command, str) and "git diff" in command:
                saw_git_diff = True
                break

        notes_text = (repo / "notes.txt").read_text()
        if "git flow acceptance complete" not in notes_text:
            raise RuntimeError("git flow task did not append the expected line to notes.txt")

        head_count_proc = run_cli_capture(
            ["git", "rev-list", "--count", "HEAD"],
            repo,
            timeout=60,
            env_overrides=task_env,
        )
        head_count_output = smoke.normalize_output(
            (head_count_proc.stdout or "") + (head_count_proc.stderr or "")
        )
        if head_count_proc.returncode != 0 or head_count_output.strip() != "2":
            raise RuntimeError(
                f"git flow task did not create a second commit\n--- output ---\n{head_count_output[:1200]}"
            )

        show_proc = run_cli_capture(
            ["git", "show", "-s", "--format=%s", "HEAD"],
            repo,
            timeout=60,
            env_overrides=task_env,
        )
        show_output = smoke.normalize_output((show_proc.stdout or "") + (show_proc.stderr or ""))
        commit_message = show_output.strip()
        if show_proc.returncode != 0 or not commit_message:
            raise RuntimeError(
                f"git flow task did not leave a readable commit message\n--- output ---\n{show_output[:1200]}"
            )
        if not re.search(r"(git|notes|acceptance|flow)", commit_message, re.IGNORECASE):
            raise RuntimeError(
                f"git flow task commit message was too generic: {commit_message!r}"
            )

        diff_proc = run_cli_capture(
            ["git", "diff", "--name-only", "HEAD~1", "HEAD"],
            repo,
            timeout=60,
            env_overrides=task_env,
        )
        diff_output = smoke.normalize_output((diff_proc.stdout or "") + (diff_proc.stderr or ""))
        changed_files = [line.strip() for line in diff_output.splitlines() if line.strip()]
        if diff_proc.returncode != 0 or "notes.txt" not in changed_files:
            raise RuntimeError(
                f"git flow task did not commit the expected file\n--- output ---\n{diff_output[:1200]}"
            )
        if not saw_git_diff:
            raise RuntimeError(
                f"git flow task stream-json did not record a Bash command containing git diff\n--- output ---\n{combined[:4000]}"
            )

        return {
            "status": "ok",
            "bash_calls": len(bash_blocks),
            "commit_message": commit_message,
            "changed_files": changed_files,
        }


def run_mossen_md_loader_canary() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-mossen-md-proj.") as proj_dir, tempfile.TemporaryDirectory(
        prefix="mossensrc-mossen-md-home."
    ) as config_dir:
        proj = Path(proj_dir)
        cfg = Path(config_dir)
        (proj / "MOSSEN.md").write_text("Project canary instruction\n")
        (cfg / "MOSSEN.md").write_text("Global canary instruction\n")
        proc = subprocess.run(
            [
                "bun",
                "-e",
                (
                    "import { enableConfigs } from './utils/config.ts'; "
                    "import { setOriginalCwd, setProjectRoot } from './bootstrap/state.ts'; "
                    "import { clearMemoryFileCaches, getMemoryFiles, getMossenMds } from './utils/mossenmd.ts'; "
                    "enableConfigs(); "
                    "setOriginalCwd(process.env.PROJ); "
                    "setProjectRoot(process.env.PROJ); "
                    "clearMemoryFileCaches(); "
                    "const files = await getMemoryFiles(); "
                    "console.log(JSON.stringify({"
                    "paths: files.map(f => ({ path: f.path, type: f.type })),"
                    "prompt: getMossenMds(files)"
                    "}));"
                ),
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
            timeout=60,
            env={
                **os.environ,
                MOSSEN_CONFIG_ENV_KEY: str(cfg),
                "PROJ": str(proj),
            },
            check=True,
        )
        payload = json.loads(smoke.normalize_output(proc.stdout.strip()))
        prompt = payload["prompt"]
        if "Global canary instruction" not in prompt or "Project canary instruction" not in prompt:
            raise RuntimeError(
                f"MOSSEN.md loader canary missing expected instructions\n--- output ---\n{prompt[:1200]}"
            )
        if prompt.index("Global canary instruction") > prompt.index("Project canary instruction"):
            raise RuntimeError(
                "MOSSEN.md loader ordering drifted: expected global before project in prompt"
            )
        return {
            "status": "ok",
            "loaded_paths": payload["paths"],
        }


def run_auto_memory_prompt_canary() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="mossensrc-auto-memory.") as memory_dir:
        memdir = Path(memory_dir)
        entry = memdir / "MEMORY.md"
        entry.write_text("Auto memory canary instruction\n")
        proc = subprocess.run(
            [
                "bun",
                "-e",
                (
                    "import { buildMemoryPrompt } from './memdir/memdir.ts'; "
                    "import { getAutoMemPath } from './memdir/paths.ts'; "
                    "const path = getAutoMemPath(); "
                    "const prompt = buildMemoryPrompt({ displayName: 'auto memory', memoryDir: path }); "
                    "console.log(JSON.stringify({ path, prompt }));"
                ),
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
            timeout=60,
            env={
                **os.environ,
                "MOSSEN_COWORK_MEMORY_PATH_OVERRIDE": str(memdir) + os.sep,
            },
            check=True,
        )
        payload = json.loads(smoke.normalize_output(proc.stdout.strip()))
        if payload["path"] != str(memdir) + os.sep:
            raise RuntimeError(
                f"auto memory path override drifted\n--- output ---\n{json.dumps(payload, ensure_ascii=False, indent=2)}"
            )
        if "Auto memory canary instruction" not in payload["prompt"]:
            raise RuntimeError("auto memory prompt missing canary entry")
        return {
            "status": "ok",
            "path": payload["path"],
        }


def run_permission_mode_canary() -> dict[str, object]:
    proc = subprocess.run(
            [
                "bun",
                "-e",
                (
                    "import { enableConfigs } from './utils/config.ts'; "
                    "import { getEmptyToolPermissionContext } from './Tool.ts'; "
                    "import { initialPermissionModeFromCLI } from './utils/permissions/permissionSetup.ts'; "
                    "import { isPathAllowed } from './utils/permissions/pathValidation.ts'; "
                    "enableConfigs(); "
                    "const base = getEmptyToolPermissionContext(); "
                    "const target = process.cwd() + '/personal-acceptance.txt'; "
                    "const mk = mode => ({ ...base, mode, additionalWorkingDirectories: new Map() }); "
                    "console.log(JSON.stringify({"
                    "modes: {"
                    "default: initialPermissionModeFromCLI({ permissionModeCli: undefined, dangerouslySkipPermissions: false }).mode,"
                    "plan: initialPermissionModeFromCLI({ permissionModeCli: 'plan', dangerouslySkipPermissions: false }).mode,"
                    "acceptEdits: initialPermissionModeFromCLI({ permissionModeCli: 'acceptEdits', dangerouslySkipPermissions: false }).mode,"
                    "bypass: initialPermissionModeFromCLI({ permissionModeCli: undefined, dangerouslySkipPermissions: true }).mode"
                    "},"
                    "writes: {"
                    "defaultWrite: isPathAllowed(target, mk('default'), 'write').allowed,"
                    "acceptEditsWrite: isPathAllowed(target, mk('acceptEdits'), 'write').allowed,"
                    "planWrite: isPathAllowed(target, mk('plan'), 'write').allowed"
                    "}"
                    "}));"
                ),
            ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=60,
        check=True,
    )
    payload = json.loads(smoke.normalize_output(proc.stdout.strip()))
    modes = payload["modes"]
    writes = payload["writes"]
    if modes != {
        "default": "default",
        "plan": "plan",
        "acceptEdits": "acceptEdits",
        "bypass": "bypassPermissions",
    }:
        raise RuntimeError(
            f"permission mode resolution drifted\n--- output ---\n{json.dumps(payload, ensure_ascii=False, indent=2)}"
        )
    if writes != {
        "defaultWrite": False,
        "acceptEditsWrite": True,
        "planWrite": False,
    }:
        raise RuntimeError(
            f"permission path gating drifted\n--- output ---\n{json.dumps(payload, ensure_ascii=False, indent=2)}"
        )
    return {
        "status": "ok",
        "modes": modes,
        "writes": writes,
    }


def run_permission_cycle_canary() -> dict[str, object]:
    proc = subprocess.run(
        [
            "bun",
            "-e",
            (
                "import { enableConfigs } from './utils/config.ts'; "
                "import { getEmptyToolPermissionContext } from './Tool.ts'; "
                "import { getNextPermissionMode, cyclePermissionMode } from './utils/permissions/getNextPermissionMode.ts'; "
                "enableConfigs(); "
                "const withFlags = ctx => ({ "
                "...ctx, "
                "mode: ctx.mode || 'default', "
                "isBypassPermissionsModeAvailable: true, "
                "isAutoModeAvailable: false "
                "}); "
                "let context = withFlags({ ...getEmptyToolPermissionContext(), mode: 'default' }); "
                "const sequence = [context.mode]; "
                "for (let i = 0; i < 4; i++) { "
                "const nextMode = getNextPermissionMode(context); "
                "const cycled = cyclePermissionMode(context); "
                "sequence.push(nextMode); "
                "context = withFlags({ ...cycled.context, mode: cycled.nextMode }); "
                "} "
                "console.log(JSON.stringify({ sequence }));"
            ),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=60,
        check=True,
    )
    payload = json.loads(smoke.normalize_output(proc.stdout.strip()))
    sequence = payload["sequence"]
    expected = [
        "default",
        "acceptEdits",
        "plan",
        "bypassPermissions",
        "default",
    ]
    if sequence != expected:
        raise RuntimeError(
            "permission cycle drifted\n"
            f"expected={expected}\n"
            f"actual={sequence}"
        )
    return {
        "status": "ok",
        "sequence": sequence,
    }


def run_full_smoke_summary() -> dict[str, object]:
    proc = subprocess.run(
        ["python3", "scripts/smoke_check.py"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=1800,
    )
    stdout = smoke.normalize_output(proc.stdout or "")
    if proc.returncode != 0:
        raise RuntimeError(
            f"smoke_check.py failed with code {proc.returncode}\n--- stderr ---\n{(proc.stderr or '')[:1200]}\n--- stdout ---\n{stdout[:1200]}"
        )
    report = json.loads(stdout)
    failed = [
        name
        for name, value in report.items()
        if isinstance(value, dict) and value.get("status") not in (None, "ok", "rendered", "blocked", "gated")
    ]
    return {
        "status": "ok",
        "total_checks": report.get("summary", {}).get("checksRun")
        or report.get("summary", {}).get("checks"),
        "failed_checks": failed,
        "domain_matrix_keys": sorted((report.get("domain_matrix") or {}).keys())[:20],
    }


def run_deep_sidechain_stress_summary(samples: int) -> dict[str, object]:
    result = smoke.run_agentic_deep_sidechain_stress_audit(samples)
    if result.get("status") == "harness_regression":
        raise RuntimeError(
            json.dumps(
                {
                    "deep_sidechain_stress_failed": True,
                    "result": result,
                },
                ensure_ascii=False,
                indent=2,
            )
        )
    return result


def run_case(
    index: int,
    total: int,
    section: str,
    name: str,
    fn: Callable[[dict[str, Any]], Any],
    context: dict[str, Any],
    report: dict[str, Any],
    store_key: Optional[str] = None,
) -> None:
    print(f"[{index:02d}/{total:02d}] {section}.{name} starting", file=sys.stderr, flush=True)
    start = time.time()
    try:
        result = fn(context)
        if store_key:
            context[store_key] = result
        report["sections"].setdefault(section, {})[name] = {
            "status": "ok",
            "durationSec": round(time.time() - start, 2),
            "result": result,
        }
        print(
            f"[{index:02d}/{total:02d}] {section}.{name} done {time.time() - start:.1f}s",
            file=sys.stderr,
            flush=True,
        )
    except Exception as exc:
        report["sections"].setdefault(section, {})[name] = {
            "status": "failed",
            "durationSec": round(time.time() - start, 2),
            "error": str(exc),
        }
        report["failures"].append({"section": section, "name": name, "error": str(exc)})
        print(
            f"[{index:02d}/{total:02d}] {section}.{name} failed {time.time() - start:.1f}s",
            file=sys.stderr,
            flush=True,
        )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Automated personal single-user acceptance suite for Mossen."
    )
    parser.add_argument(
        "--with-full-smoke",
        action="store_true",
        help="Also run scripts/smoke_check.py as a final gate.",
    )
    parser.add_argument(
        "--samples",
        type=int,
        default=2,
        help="Sample count for deterministic branch stability audit.",
    )
    parser.add_argument(
        "--with-deep-sidechain-stress",
        action="store_true",
        help="Also run the non-blocking deep sidechain stress suite and report pass/provider_transport_unstable/harness_regression.",
    )
    parser.add_argument(
        "--with-extended-real-tasks",
        action="store_true",
        help="Also run optional extended real-task acceptance, including the local git-flow task.",
    )
    args = parser.parse_args()
    reap_stale_personal_acceptance_processes()
    smoke.reap_stale_smoke_processes()
    register_personal_acceptance_pid()

    try:
        report: dict[str, Any] = {
            "scope": "personal-single-user",
            "status": "ok",
            "sections": {},
            "failures": [],
        }
        context: dict[str, Any] = {}

        steps: list[tuple[str, str, Callable[[dict[str, Any]], Any], str | None]] = [
            ("baseline", "auth_status_text", lambda _ctx: run_auth_status_text_smoke(), None),
            ("baseline", "platform_check", lambda _ctx: run_platform_check_json(), "platform_check"),
            ("slash_commands", "inventory_audit", lambda _ctx: run_slash_inventory_audit(), "slash_inventory"),
            ("slash_commands", "help_matrix", lambda _ctx: smoke.run_help_matrix(), None),
            ("slash_commands", "statusline", lambda _ctx: smoke.run_statusline_smoke(), None),
            ("slash_commands", "memory", lambda _ctx: run_memory_command_smoke(), None),
            ("slash_commands", "permissions", lambda _ctx: run_permissions_command_smoke(), None),
            ("slash_commands", "doctor", lambda _ctx: smoke.run_doctor_smoke(), None),
            ("slash_commands", "config", lambda _ctx: run_slash_ui_probe("/config"), None),
        (
            "slash_commands",
            "config_default_permission_mode_surface",
            lambda _ctx: run_config_default_permission_mode_surface_smoke(),
            None,
        ),
        ("slash_commands", "model", lambda _ctx: run_slash_ui_probe("/model"), None),
        ("slash_commands", "status", lambda _ctx: run_slash_ui_probe("/status"), None),
        ("slash_commands", "stats", lambda _ctx: run_slash_ui_probe("/stats"), None),
        ("slash_commands", "skills", lambda _ctx: run_slash_ui_probe("/skills"), None),
        ("slash_commands", "hooks", lambda _ctx: run_slash_ui_probe("/hooks"), None),
        ("slash_commands", "tasks", lambda _ctx: run_slash_ui_probe("/tasks"), None),
        ("slash_commands", "theme", lambda _ctx: run_slash_ui_probe("/theme"), None),
        ("slash_commands", "plugin", lambda _ctx: run_slash_ui_probe("/plugin"), None),
        ("slash_commands", "sandbox", lambda _ctx: run_slash_ui_probe("/sandbox"), None),
        ("slash_commands", "agents", lambda _ctx: run_slash_ui_probe("/agents"), None),
        ("slash_commands", "mcp", lambda _ctx: run_slash_ui_probe("/mcp"), None),
        ("slash_commands", "plan", lambda _ctx: run_plan_command_smoke(), None),
        ("slash_commands", "effort", lambda _ctx: run_slash_ui_probe("/effort"), None),
        ("slash_commands", "profile", lambda _ctx: run_slash_ui_probe("/profile"), None),
        ("slash_commands", "context", lambda _ctx: run_slash_ui_probe("/context"), None),
        ("slash_commands", "cost", lambda _ctx: run_slash_ui_probe("/cost"), None),
        ("slash_commands", "feedback", lambda _ctx: run_slash_ui_probe("/feedback"), None),
        ("slash_commands", "color", lambda _ctx: run_color_command_smoke(), None),
        (
            "tools",
            "prompt",
            lambda _ctx: run_self_audit_in_subprocess("run_prompt_smoke", timeout=180),
            None,
        ),
        (
            "tools",
            "tool_use",
            lambda _ctx: run_self_audit_in_subprocess("run_tool_use_smoke", timeout=300),
            None,
        ),
        ("tools", "local_git_runtime", lambda _ctx: smoke.run_local_git_runtime_audit(), None),
        (
            "tools",
            "agentic_tool_loop",
            lambda _ctx: run_smoke_audit_in_subprocess(
                "run_agentic_tool_loop_canary_audit",
                timeout=120,
            ),
            None,
        ),
        ("prompt_injection", "mossen_md_loader", lambda _ctx: run_mossen_md_loader_canary(), None),
        ("compaction", "context_pressure", lambda _ctx: smoke.run_context_pressure_runtime_audit(), None),
        ("compaction", "context_observability", lambda _ctx: run_context_observability_smoke(), None),
        ("compaction", "compact_surface_consistency", lambda _ctx: run_compact_surface_consistency_smoke(), None),
        ("compaction", "compaction_runtime", lambda _ctx: smoke.run_compaction_runtime_audit(), None),
        ("memory", "auto_memory_prompt", lambda _ctx: run_auto_memory_prompt_canary(), None),
        (
            "memory",
            "session_resume",
            lambda ctx: smoke.run_session_resume_smoke(smoke.get_project_session_dir(ctx["platform_check"])),
            None,
        ),
        ("permissions", "surface_audit", lambda _ctx: smoke.run_permission_override_surface_audit(), None),
        ("permissions", "mode_canary", lambda _ctx: run_permission_mode_canary(), None),
        ("permissions", "cycle_canary", lambda _ctx: run_permission_cycle_canary(), None),
        (
            "permissions",
            "shift_tab_runtime_cycle",
            lambda _ctx: run_permission_shift_tab_runtime_cycle_smoke(),
            None,
        ),
        ("permissions", "plan_surface_consistency", lambda _ctx: run_plan_surface_consistency_smoke(), None),
        ("permissions", "dialog_surface_consistency", lambda _ctx: run_permissions_surface_consistency_smoke(), None),
        (
            "permissions",
            "accept_edits_surface_consistency",
            lambda _ctx: run_permissions_accept_edits_surface_consistency_smoke(),
            None,
        ),
        (
            "permissions",
            "bypass_surface_consistency",
            lambda _ctx: run_permissions_bypass_surface_consistency_smoke(),
            None,
        ),
        (
            "permissions",
            "default_permission_mode_runtime_consistency",
            lambda _ctx: run_default_permission_mode_runtime_consistency_smoke(),
            None,
        ),
        ("ui", "plugin_list", lambda _ctx: smoke.run_plugin_list_smoke(), None),
        ("ui", "agents_list", lambda _ctx: smoke.run_agents_smoke(), None),
        ("ui", "chat_flow", lambda _ctx: smoke.run_chat_smoke(), None),
        ("ui", "interactive_language_runtime", lambda _ctx: smoke.run_interactive_language_runtime_audit(), None),
        ("ui", "status_observability", lambda _ctx: run_status_observability_smoke(), None),
        ("ui", "status_profile_surface", lambda _ctx: run_status_profile_surface_smoke(), None),
        ("ui", "status_local_tier_surface", lambda _ctx: run_status_local_tier_surface_smoke(), None),
        (
            "ui",
            "auth_status_surface_consistency",
            lambda _ctx: run_auth_status_surface_consistency_smoke(),
            None,
        ),
        (
            "ui",
            "statusline_surface_consistency",
            lambda _ctx: run_statusline_surface_consistency_smoke(),
            None,
        ),
        (
            "ui",
            "worktree_status_surface_consistency",
            lambda _ctx: run_worktree_status_surface_consistency_smoke(),
            None,
        ),
        (
            "ui",
            "resume_worktree_selector_consistency",
            lambda _ctx: run_resume_worktree_selector_consistency_smoke(),
            None,
        ),
        (
            "ui",
            "resume_title_current_worktree_preference",
            lambda _ctx: run_resume_title_current_worktree_preference_smoke(),
            None,
        ),
        (
            "ui",
            "resume_startup_worktree_switch_consistency",
            lambda _ctx: run_resume_startup_worktree_switch_consistency_smoke(),
            None,
        ),
        (
            "ui",
            "resume_worktree_session_storage_consistency",
            lambda _ctx: run_resume_worktree_session_storage_consistency_smoke(),
            None,
        ),
        (
            "ui",
            "worktree_ide_open_surface_consistency",
            lambda _ctx: run_worktree_ide_open_surface_consistency_smoke(),
            None,
        ),
        (
            "ui",
            "worktree_dev_flow_surface_consistency",
            lambda _ctx: run_worktree_dev_flow_surface_consistency_smoke(),
            None,
        ),
        ("ui", "statusline_runtime_surface", lambda _ctx: run_statusline_runtime_surface_smoke(), None),
        (
            "ui",
            "statusline_runtime_surface_compact",
            lambda _ctx: run_statusline_runtime_surface_compact_smoke(),
            None,
        ),
        (
            "ui",
            "statusline_runtime_surface_local_tier",
            lambda _ctx: run_statusline_runtime_surface_local_tier_smoke(),
            None,
        ),
        ("local_commands", "clear", lambda _ctx: run_clear_command_smoke(), None),
        (
            "local_commands",
            "clear_history_reset",
            lambda _ctx: run_clear_history_reset_smoke(),
            None,
        ),
        ("local_commands", "compact", lambda _ctx: run_compact_command_smoke(), None),
        ("local_commands", "diff", lambda _ctx: run_diff_command_smoke(), None),
        ("local_commands", "model_set", lambda _ctx: run_model_set_command_smoke(), None),
        (
            "local_commands",
            "model_surface_consistency",
            lambda _ctx: run_model_surface_consistency_smoke(),
            None,
        ),
        ("local_commands", "keybindings", lambda _ctx: run_keybindings_command_smoke(), None),
        ("local_commands", "theme", lambda _ctx: run_theme_command_persistence_smoke(), None),
        ("local_commands", "export", lambda _ctx: run_export_command_smoke(), None),
        ("local_commands", "color", lambda _ctx: run_color_command_persistence_smoke(), None),
        ("local_commands", "rename", lambda _ctx: run_rename_command_smoke(), None),
        ("local_commands", "branch", lambda _ctx: run_branch_command_smoke(), None),
        ("local_commands", "resume", lambda _ctx: run_resume_command_smoke(), None),
        ("local_commands", "rewind", lambda _ctx: run_rewind_command_smoke(), None),
        ("profiles", "profile_behavior", lambda _ctx: run_profile_behavior_smoke(), None),
        ("profiles", "profile_command_persistence", lambda _ctx: run_profile_command_persistence_smoke(), None),
        ("profiles", "profile_surface_consistency", lambda _ctx: run_profile_surface_consistency_smoke(), None),
        ("profiles", "effort_command_persistence", lambda _ctx: run_effort_command_persistence_smoke(), None),
        ("profiles", "model_tier_local_surface", lambda _ctx: run_model_tier_local_surface_smoke(), None),
        ("real_tasks", "bugfix_task", lambda _ctx: run_bugfix_task_smoke(), None),
        ("real_tasks", "worktree_task", lambda _ctx: run_worktree_task_smoke(), None),
        ]
        if args.with_deep_sidechain_stress:
            steps.append(
                (
                    "tools",
                    "deep_sidechain_stress",
                    lambda _ctx: run_deep_sidechain_stress_summary(args.samples),
                    None,
                )
            )
        if args.with_extended_real_tasks:
            steps.append(
                (
                    "real_tasks",
                    "git_flow_task",
                    lambda _ctx: run_git_flow_task_smoke(),
                    None,
                )
            )
            steps.append(
                (
                    "real_tasks",
                    "multi_worktree_task",
                    lambda _ctx: run_multi_worktree_task_smoke(),
                    None,
                )
            )
        if args.with_full_smoke:
            steps.append(("final_gate", "full_smoke", lambda _ctx: run_full_smoke_summary(), None))

        total = len(steps)
        for index, (section, name, fn, store_key) in enumerate(steps, start=1):
            run_case(index, total, section, name, fn, context, report, store_key=store_key)

        report["summary"] = {
            "checksRun": total,
            "failed": len(report["failures"]),
            "passed": total - len(report["failures"]),
        }
        if report["failures"]:
            report["status"] = "failed"

        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 1 if report["failures"] else 0
    finally:
        unregister_personal_acceptance_pid()


if __name__ == "__main__":
    raise SystemExit(main())
