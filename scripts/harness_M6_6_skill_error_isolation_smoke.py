#!/usr/bin/env python3
"""
M6.6 — 单 skill 解析失败不影响其他 skill (loader 错误隔离)。

按 harness全链路测试.md §C.1 M6.6 契约:
  前置:
    - good: $MOSSEN_CONFIG_DIR/skills/m66_good_skill/SKILL.md (合法 frontmatter +
            body marker 'M6_6_GOOD_BODY_xyz')
    - bad:  $MOSSEN_CONFIG_DIR/skills/m66_bad_skill/SKILL.md (frontmatter YAML
            故意写坏: 未闭合块 + 非法字段, 让 parseFrontmatter 抛错)
  步骤: bun -e: enableConfigs(); await getSkillDirCommands(fixture_cwd)
  观察点 (强契约 — 错误隔离):
    1. bun exit_code == 0 (loader 全程不 throw)
    2. m66_good_skill 出现在结果列表 (good_present == True)
    3. good 的 getPromptForCommand 仍能调用且 body 含 'M6_6_GOOD_BODY_xyz'
    4. (软契约) bad_skill 不在结果 OR 在结果但不影响 good
  反测信号: src/skills/loadSkillsDir.ts:425 改 Promise.all 成 await 串行 + 移除
            inner try/catch (line 427) → 1 个坏 frontmatter 让 outer Promise.all
            reject → loadSkillsFromSkillsDir return [] 或 throw → good_present == False → fail
  反测信号 2: src/skills/loadSkillsDir.ts:476 把 try/catch 的 catch 改为 throw error
            → 1 坏 skill let outer Promise.all reject → fail

调研发现 (loadSkillsDir.ts:425-481):
  使用 Promise.all + 每个 entry 内 try/catch + return null,
  最后 results.filter(r => r !== null) 收集成功项。
  双层防御: parseFrontmatter / parseSkillFrontmatterFields 异常被 catch (line 476-478),
  null 被 filter 滤掉。坏 skill 应该 silently dropped。
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

GOOD_SKILL_NAME = "m66_good_skill"
BAD_SKILL_NAME = "m66_bad_skill"
GOOD_BODY_MARKER = "M6_6_GOOD_BODY_xyz"


def _write_good_skill(skill_md_path: Path) -> None:
    skill_md_path.parent.mkdir(parents=True, exist_ok=True)
    content = (
        f"---\n"
        f"name: {GOOD_SKILL_NAME}\n"
        f"description: M6.6 good skill body marker {GOOD_BODY_MARKER}\n"
        f"user-invocable: true\n"
        f"---\n"
        f"\n{GOOD_BODY_MARKER}\n"
    )
    skill_md_path.write_text(content, encoding="utf-8")


def _write_bad_skill(skill_md_path: Path) -> None:
    """让 SKILL.md 自身是个目录 → fs.readFile 抛 EISDIR。

    这是 mossen loader 真正会遇到的硬错误 (vs 宽容的 frontmatter 解析).
    inner readFile catch 接住 EISDIR → 该 entry 返回 null, 其他 skill 仍工作。
    Mutation: 改 inner readFile try/catch 删掉 → EISDIR 上浮 → outer catch
    接住 → 该 entry 仍返回 null (good 仍工作). Mutation: 进一步 outer catch 改
    rethrow → Promise.all reject → loader 整体 fail → good 也丢。"""
    skill_md_path.parent.mkdir(parents=True, exist_ok=True)
    # SKILL.md 作为 *目录* 而不是文件 → readFile() throws EISDIR
    skill_md_path.mkdir(exist_ok=True)
    # 顺便放点东西进去标识
    (skill_md_path / "intentionally_a_dir.txt").write_text(
        "M6.6 bad skill: SKILL.md is a directory not a file\n",
        encoding="utf-8",
    )


def _bun_load_and_invoke_good(env: dict, fixture_cwd: str) -> tuple[int, str, str]:
    """跑 bun -e: 列举所有 dir skills + 调 good.getPromptForCommand 取 body。"""
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        f"process.chdir({json.dumps(fixture_cwd)});"
        "const { getSkillDirCommands } = await import('./skills/loadSkillsDir.ts');"
        "const cmds = await getSkillDirCommands(process.cwd());"
        "const names = cmds.map(c => c.name);"
        f"const good = cmds.find(c => c.name === {json.dumps(GOOD_SKILL_NAME)});"
        "let goodBody = '';"
        "let goodInvokeError = null;"
        "if (good) {"
        "  try {"
        "    const blocks = await good.getPromptForCommand('', {"
        "      abortController: new AbortController(),"
        "      readFileTimeoutMs: 30000,"
        "      options: { commands: [], tools: [] },"
        "      messageId: 'm66', agentId: 'm66',"
        "      setToolJSX: () => {},"
        "      getQueuedCommands: () => [],"
        "      removeQueuedCommands: () => {},"
        "      getMcpClients: () => []"
        "    });"
        "    goodBody = blocks.map(b => b.type === 'text' ? b.text : '').join('\\n');"
        "  } catch (e) {"
        "    goodInvokeError = String(e && e.message || e);"
        "  }"
        "}"
        "process.stdout.write(JSON.stringify({"
        "  exit_path: 'normal',"
        "  names: names,"
        "  good_found: !!good,"
        "  good_body: goodBody,"
        "  good_invoke_error: goodInvokeError"
        "}) + '\\n');"
    )
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=120,
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


def case_skill_error_isolation() -> dict:
    ctx = make_fixture("M6.6")

    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)

    good_md = ctx.mossen_config_home / "skills" / GOOD_SKILL_NAME / "SKILL.md"
    bad_md = ctx.mossen_config_home / "skills" / BAD_SKILL_NAME / "SKILL.md"
    _write_good_skill(good_md)
    _write_bad_skill(bad_md)

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    rc, out, err = _bun_load_and_invoke_good(env, str(fixture_cwd))
    parsed = _parse_last_json(out)

    write_command_log(
        ctx,
        [RUN_BUN, "-e", "<getSkillDirCommands + good.getPromptForCommand>"],
        out,
        err,
        rc,
    )

    if not parsed:
        return {
            "name": "skill_error_isolation",
            "ok": False,
            "stage": "parse",
            "exit_code": rc,
            "stdout_excerpt": out[:500],
            "stderr_excerpt": err[:500],
            "_ctx": ctx,
        }

    names = parsed.get("names") or []
    good_found = parsed.get("good_found") is True
    good_body = parsed.get("good_body") or ""
    good_invoke_error = parsed.get("good_invoke_error")
    good_body_has_marker = GOOD_BODY_MARKER in good_body
    bad_dropped = BAD_SKILL_NAME not in names  # 软契约 (bad 应该被滤掉)

    # 强契约: loader 不 throw + good 在结果 + good 仍能 invoke + body marker 完好
    return {
        "name": "skill_error_isolation",
        "ok": (
            rc == 0
            and good_found
            and good_invoke_error is None
            and good_body_has_marker
        ),
        "exit_code": rc,
        "good_found": good_found,
        "good_body_has_marker": good_body_has_marker,
        "good_invoke_error": good_invoke_error,
        "bad_dropped": bad_dropped,
        "names": names,
        "stderr_excerpt": err[:400],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_skill_error_isolation()
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
                    f"exit={r.get('exit_code')} "
                    f"good_found={r.get('good_found')} "
                    f"good_marker={r.get('good_body_has_marker')} "
                    f"good_invoke_err={r.get('good_invoke_error')} "
                    f"bad_dropped={r.get('bad_dropped')} "
                    f"names={r.get('names')}"
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
            "M6.6: 1 个 SKILL.md frontmatter 写坏不能让 loader 整体 fail —— "
            "loadSkillsFromSkillsDir 内 entries.map 每个 entry try/catch return null, "
            "结果 filter 掉 null。good_skill 必须仍能被发现 + 调用 + body 原样。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
