# Mossen Slash Command 全量 Inventory

- **95** 条当前用户可见入口：82 live commands + 13 userInvocable skills
- **18** 条硬切后遗留的 stub 命令 (`isEnabled: () => false` + `isHidden: true`)
- **1** 个 `userInvocable: false` 的 bundled skill（agent 内部使用，不暴露为 slash 命令）
- **合计 114** 条 registered entries

自动生成。更新命令：

```bash
python3 scripts/command_inventory.py > docs/command_inventory.md
```

架构参考：MOSSEN.md §3.4（两条注册路径）、§3.5（七种可见性门）、`utils/deferredSlashCommands.ts`、`utils/customBackend.ts`。

待人工补填（P1-5 slice B，用户本机完成）的四列：

- **依赖 CB**：是否依赖 custom backend 才能工作 (Y/N)
- **依赖本地**：是否依赖本地文件 / git / worktree (Y/N)
- **依赖 hosted**：是否依赖 hosted 服务 (Y/N)
- **结论**：保留 / hidden / deferred / 其他（含 bug 单链接）

## 1. Live `commands/` — 用户可见或条件可见

共 82 条。

| # | 命令 | 别名 | 路径 | 可见性门 | 描述 | 依赖CB | 依赖本地 | 依赖hosted | 结论 |
|---|------|------|------|---------|------|:-----:|:-------:|:---------:|------|
| 1 | `/add-dir` | — | `commands/add-dir/index.ts` | — | Add a new working directory |  |  |  |  |
| 2 | `/advisor` | — | `commands/advisor.ts` | isHidden=getter; isEnabled=fn | Configure the advisor model |  |  |  |  |
| 3 | `/agents` | — | `commands/agents/index.ts` | — | Manage agent configurations |  |  |  |  |
| 4 | `/assistant` | — | `commands/assistant/index.ts` | isEnabled=fn; deferred=assistant | Connect to a running assistant session |  |  |  |  |
| 5 | `/branch` | — | `commands/branch/index.ts` | feature=FORK_SUBAGENT | Create a branch of the current conversation at this point |  |  |  |  |
| 6 | `/brief` | — | `commands/brief.ts` | isEnabled=fn; feature=KAIROS,KAIROS_BRIEF | Toggle brief-only mode |  |  |  |  |
| 7 | `/btw` | — | `commands/btw/index.ts` | — | Ask a quick side question without interrupting the main conversation |  |  |  |  |
| 8 | `/chrome` | — | `commands/chrome/index.ts` | isHidden=getter; isEnabled=fn | ${getProductDisplayName()} in Chrome (Beta) settings |  |  |  |  |
| 9 | `/clear` | `/reset`, `/new` | `commands/clear/index.ts` | — | Clear conversation history and free up context |  |  |  |  |
| 10 | `/color` | — | `commands/color/index.ts` | — | Set the prompt bar color for this session |  |  |  |  |
| 11 | `/commit` | — | `commands/commit.ts` | — | Create a git commit |  |  |  |  |
| 12 | `/commit-push-pr` | — | `commands/commit-push-pr.ts` | — | Commit, push, and open a PR |  |  |  |  |
| 13 | `/compact` | — | `commands/compact/index.ts` | isEnabled=fn | Clear conversation history but keep a summary in context. Optional: /compact [instructions for summarization] |  |  |  |  |
| 14 | `/config` | `/settings` | `commands/config/index.ts` | — | Open config panel |  |  |  |  |
| 15 | `/context` | — | `commands/context/index.ts` | isHidden=getter; isEnabled=fn | Visualize current context usage as a colored grid |  |  |  |  |
| 16 | `/copy` | — | `commands/copy/index.ts` | — | Copy Mossen |  |  |  |  |
| 17 | `/cost` | — | `commands/cost/index.ts` | isHidden=getter | Show the total cost and duration of the current session |  |  |  |  |
| 18 | `/desktop` | `/app` | `commands/desktop/index.ts` | isHidden=getter; availability=getter; isEnabled=fn | Continue the current session in the desktop companion app |  |  |  |  |
| 19 | `/diff` | — | `commands/diff/index.ts` | — | View uncommitted changes and per-turn diffs |  |  |  |  |
| 20 | `/doctor` | — | `commands/doctor/index.ts` | isEnabled=fn | Diagnose and verify your ${getProductDisplayName()} installation and settings |  |  |  |  |
| 21 | `/effort` | — | `commands/effort/index.ts` | — | Set effort level for model usage |  |  |  |  |
| 22 | `/exit` | `/quit` | `commands/exit/index.ts` | — | Exit the REPL |  |  |  |  |
| 23 | `/export` | — | `commands/export/index.ts` | — | Export the current conversation to a file or clipboard |  |  |  |  |
| 24 | `/extra-usage` | — | `commands/extra-usage/index.ts` | isHidden=getter; isEnabled=fn | — |  |  |  |  |
| 25 | `/fast` | — | `commands/fast/index.ts` | isHidden=getter; availability=['hosted', 'console']; isEnabled=fn | Toggle fast mode (${FAST_MODE_MODEL_DISPLAY} only) |  |  |  |  |
| 26 | `/feedback` | `/bug` | `commands/feedback/index.ts` | isEnabled=fn | Submit feedback about Mossen |  |  |  |  |
| 27 | `/files` | — | `commands/files/index.ts` | isEnabled=fn | List all files currently in context |  |  |  |  |
| 28 | `/heapdump` | — | `commands/heapdump/index.ts` | isHidden=true; isEnabled=fn; deferred=heapdump | Dump the JS heap to ~/Desktop |  |  |  |  |
| 29 | `/help` | — | `commands/help/index.ts` | — | Show help and available commands |  |  |  |  |
| 30 | `/hooks` | — | `commands/hooks/index.ts` | — | View hook configurations for tool events |  |  |  |  |
| 31 | `/ide` | — | `commands/ide/index.ts` | — | Manage IDE integrations and show status |  |  |  |  |
| 32 | `/init` | — | `commands/init.ts` | feature=NEW_INIT | — |  |  |  |  |
| 33 | `/init-verifiers` | — | `commands/init-verifiers.ts` | — | Create verifier skill(s) for automated verification of code changes |  |  |  |  |
| 34 | `/insights` | — | `commands/insights.ts` | — | Generate a report analyzing your ${getProductDisplayName()} sessions |  |  |  |  |
| 35 | `/install` | — | `commands/install.tsx` | — | Install the ${getProductDisplayName()} native build |  |  |  |  |
| 36 | `/install-github-app` | — | `commands/install-github-app/index.ts` | isHidden=getter; availability=getter; isEnabled=fn | — |  |  |  |  |
| 37 | `/install-slack-app` | — | `commands/install-slack-app/index.ts` | availability=['hosted'] | Install the Mossen Slack app |  |  |  |  |
| 38 | `/keybindings` | — | `commands/keybindings/index.ts` | isEnabled=fn | Open or create your keybindings configuration file |  |  |  |  |
| 39 | `/lang` | — | `commands/lang/index.ts` | — | Quickly switch interface language |  |  |  |  |
| 40 | `/login` | — | `commands/login/index.ts` | isEnabled=fn | Show Mossen backend credential setup guidance |  |  |  |  |
| 41 | `/logout` | — | `commands/logout/index.ts` | isEnabled=fn | Clear locally cached auth state for the current backend |  |  |  |  |
| 42 | `/mcp` | — | `commands/mcp/index.ts` | — | Manage MCP servers |  |  |  |  |
| 43 | `/memory` | — | `commands/memory/index.ts` | — | Edit Mossen memory files |  |  |  |  |
| 44 | `/mobile` | `/ios`, `/android` | `commands/mobile/index.ts` | isEnabled=fn | Show QR code to download the Mossen mobile app |  |  |  |  |
| 45 | `/model` | — | `commands/model/index.ts` | — | Set the AI model for ${getProductAssistantName()} (currently ${renderModelName(getMainLoopModel())}) |  |  |  |  |
| 46 | `/output-style` | — | `commands/output-style/index.ts` | isHidden=true; isEnabled=fn; deferred=output-style | Deprecated: use /config to change output style |  |  |  |  |
| 47 | `/passes` | — | `commands/passes/index.ts` | isHidden=getter; isEnabled=fn; deferred=passes | Share a free week of ${getProductDisplayName()} with friends and earn extra usage |  |  |  |  |
| 48 | `/permissions` | `/allowed-tools` | `commands/permissions/index.ts` | — | Manage allow & deny tool permission rules |  |  |  |  |
| 49 | `/plan` | — | `commands/plan/index.ts` | — | Enable plan mode or view the current session plan |  |  |  |  |
| 50 | `/plugin` | `/plugins`, `/marketplace` | `commands/plugin/index.tsx` | — | Manage ${getProductDisplayName()} plugins |  |  |  |  |
| 51 | `/pr-comments` | — | `commands/pr_comments/index.ts` | deferred=pr-comments | Get comments from a GitHub pull request |  |  |  |  |
| 52 | `/privacy-settings` | — | `commands/privacy-settings/index.ts` | isEnabled=fn | View privacy and data controls for the current backend |  |  |  |  |
| 53 | `/proactive` | — | `commands/proactive.ts` | isEnabled=fn; deferred=proactive | Toggle proactive autonomous mode |  |  |  |  |
| 54 | `/profile` | — | `commands/profile/index.ts` | — | Set execution and reasoning profiles for personal workflows |  |  |  |  |
| 55 | `/rate-limit-options` | — | `commands/rate-limit-options/index.ts` | isHidden=true; isEnabled=fn | Show options when rate limit is reached |  |  |  |  |
| 56 | `/release-notes` | — | `commands/release-notes/index.ts` | isEnabled=fn; deferred=release-notes | View release notes |  |  |  |  |
| 57 | `/reload-plugins` | — | `commands/reload-plugins/index.ts` | — | Activate pending plugin changes in the current session |  |  |  |  |
| 58 | `/remote-env` | — | `commands/remote-env/index.ts` | isHidden=getter; isEnabled=fn | Configure the default remote environment for teleport sessions |  |  |  |  |
| 59 | `/web-setup` | — | `commands/remote-setup/index.ts` | isHidden=getter; availability=getter; isEnabled=fn; gb=tengu_cobalt_lantern | Set up hosted remote workspaces and GitHub access |  |  |  |  |
| 60 | `/rename` | — | `commands/rename/index.ts` | — | Rename the current conversation |  |  |  |  |
| 61 | `/resume` | `/continue` | `commands/resume/index.ts` | — | Resume a previous conversation |  |  |  |  |
| 62 | `/review` | — | `commands/review.ts` | — | Review a pull request |  |  |  |  |
| 63 | `/rewind` | `/checkpoint` | `commands/rewind/index.ts` | — | Restore the code and/or conversation to a previous point |  |  |  |  |
| 64 | `/sandbox` | — | `commands/sandbox-toggle/index.ts` | isHidden=getter | — |  |  |  |  |
| 65 | `/security-review` | — | `commands/security-review.ts` | — | Complete a security review of the pending changes on the current branch |  |  |  |  |
| 66 | `/session` | `/remote` | `commands/session/index.ts` | isHidden=getter; isEnabled=fn | Show remote session URL and QR code |  |  |  |  |
| 67 | `/skills` | — | `commands/skills/index.ts` | — | List available skills |  |  |  |  |
| 68 | `/stats` | — | `commands/stats/index.ts` | — | Show your ${getProductDisplayName()} usage statistics and activity |  |  |  |  |
| 69 | `/status` | — | `commands/status/index.ts` | — | Show ${getProductAssistantName()} status including version, model, backend, API connectivity, and tool statuses |  |  |  |  |
| 70 | `/statusline` | — | `commands/statusline.tsx` | — | Set up the ${getProductDisplayName()} status line UI |  |  |  |  |
| 71 | `/stickers` | — | `commands/stickers/index.ts` | isEnabled=fn; deferred=stickers | Order ${getProductDisplayName()} stickers |  |  |  |  |
| 72 | `/tag` | — | `commands/tag/index.ts` | isEnabled=fn | Toggle a searchable tag on the current session |  |  |  |  |
| 73 | `/tasks` | `/bashes` | `commands/tasks/index.ts` | — | List and manage background tasks |  |  |  |  |
| 74 | `/terminal-setup` | — | `commands/terminalSetup/index.ts` | — | — |  |  |  |  |
| 75 | `/theme` | — | `commands/theme/index.ts` | — | Change the theme |  |  |  |  |
| 76 | `/think-back` | — | `commands/thinkback/index.ts` | isEnabled=fn | Your 2025 ${getProductDisplayName()} year in review |  |  |  |  |
| 77 | `/thinkback-play` | — | `commands/thinkback-play/index.ts` | isHidden=true; isEnabled=fn | Play the thinkback animation |  |  |  |  |
| 78 | `/upgrade` | — | `commands/upgrade/index.ts` | availability=getter; isEnabled=fn | Open plan and billing options for the current backend |  |  |  |  |
| 79 | `/usage` | — | `commands/usage/index.ts` | availability=['hosted'] | Show plan usage limits |  |  |  |  |
| 80 | `/version` | — | `commands/version.ts` | isEnabled=fn | Print the version this session is running (not what autoupdate downloaded) |  |  |  |  |
| 81 | `/vim` | — | `commands/vim/index.ts` | — | Toggle between Vim and Normal editing modes |  |  |  |  |
| 82 | `/voice` | — | `commands/voice/index.ts` | isHidden=getter; isEnabled=fn; deferred=voice | Toggle voice mode |  |  |  |  |

## 2. `skills/bundled/*.ts` — userInvocable: true

共 13 条。这些走 skills 注册路径，不在 commands/ 目录；P0 复查时误判过（MOSSEN.md §3.4）。

| # | 命令 | 路径 | 门控 | 描述 | 依赖CB | 依赖本地 | 依赖hosted | 结论 |
|---|------|------|------|------|:-----:|:-------:|:---------:|------|
| 1 | `/batch` | `skills/bundled/batch.ts` | — | Research and plan a large-scale change, then execute it in parallel across 5–30 isolated worktree agents that each open a PR. |  |  |  |  |
| 2 | `/debug` | `skills/bundled/debug.ts` | — | — |  |  |  |  |
| 3 | `/dream` | `skills/bundled/dream.ts` | isEnabled=fn | Consolidate recent session learning into durable memory files and refresh the memory index. |  |  |  |  |
| 4 | `/loop` | `skills/bundled/loop.ts` | isEnabled=fn | Run a prompt or slash command on a recurring interval (e.g. /loop 5m /foo, defaults to 10m) |  |  |  |  |
| 5 | `/lorem-ipsum` | `skills/bundled/loremIpsum.ts` | — | Generate filler text for long context testing. Specify token count as argument (e.g., /lorem-ipsum 50000). Outputs approximately the requested number of tokens. |  |  |  |  |
| 6 | `/mossen-api` | `skills/bundled/mossenApi.ts` | — | Build apps with the Mossen API or Mossen-compatible SDKs.\n |  |  |  |  |
| 7 | `/mossen-in-chrome` | `skills/bundled/mossenInChrome.ts` | isEnabled=fn | Automates your Chrome browser to interact with web pages - clicking elements, filling forms, capturing screenshots, reading console logs, and navigating sites.  |  |  |  |  |
| 8 | `/remember` | `skills/bundled/remember.ts` | isEnabled=fn | Review auto-memory entries and propose promotions to MOSSEN.md, MOSSEN.local.md, or shared memory. Also detects outdated, conflicting, and duplicate entries acr |  |  |  |  |
| 9 | `/simplify` | `skills/bundled/simplify.ts` | — | Review changed code for reuse, quality, and efficiency, then fix any issues found. |  |  |  |  |
| 10 | `/skillify` | `skills/bundled/skillify.ts` | — | Capture this session |  |  |  |  |
| 11 | `/update-config` | `skills/bundled/updateConfig.ts` | — | Use this skill to configure the Mossen harness via settings.json. Automated behaviors ( |  |  |  |  |
| 12 | `/verify` | `skills/bundled/verify.ts` | — | Verify a code change does what it should by running the app. |  |  |  |  |

## 3. Stub `commands/*/index.js` — 硬切后遗留

共 18 条。模式 `export default { isEnabled: () => false, isHidden: true, name: '...' }`。这些是官方原来有、Mossen 硬切时为了保留 import 链不炸才留的 placeholder；用户永远看不到，可作为hidden 命令清单的审计参照（未来要么补 Mossen 版，要么彻底删）。

| # | 目录 | 内部 name | 路径 |
|---|------|-----------|------|
| 1 | `commands/ant-trace/` | `stub` | `commands/ant-trace/index.js` |
| 2 | `commands/autofix-pr/` | `stub` | `commands/autofix-pr/index.js` |
| 3 | `commands/backfill-sessions/` | `stub` | `commands/backfill-sessions/index.js` |
| 4 | `commands/break-cache/` | `stub` | `commands/break-cache/index.js` |
| 5 | `commands/bughunter/` | `stub` | `commands/bughunter/index.js` |
| 6 | `commands/ctx_viz/` | `stub` | `commands/ctx_viz/index.js` |
| 7 | `commands/debug-tool-call/` | `stub` | `commands/debug-tool-call/index.js` |
| 8 | `commands/env/` | `stub` | `commands/env/index.js` |
| 9 | `commands/good-mossen/` | `good-mossen` | `commands/good-mossen/index.js` |
| 10 | `commands/issue/` | `stub` | `commands/issue/index.js` |
| 11 | `commands/mock-limits/` | `stub` | `commands/mock-limits/index.js` |
| 12 | `commands/oauth-refresh/` | `stub` | `commands/oauth-refresh/index.js` |
| 13 | `commands/onboarding/` | `stub` | `commands/onboarding/index.js` |
| 14 | `commands/perf-issue/` | `stub` | `commands/perf-issue/index.js` |
| 15 | `commands/reset-limits/` | `stub` | `commands/reset-limits/index.js` |
| 16 | `commands/share/` | `stub` | `commands/share/index.js` |
| 17 | `commands/summary/` | `stub` | `commands/summary/index.js` |
| 18 | `commands/teleport/` | `stub` | `commands/teleport/index.js` |

## 4. `skills/bundled/*.ts` — userInvocable: false（非用户入口）

共 1 条。这些 skill 只被 agent 内部或其他代码调用，不暴露为 slash 命令。

| # | 名称 | 路径 | 描述 |
|---|------|------|------|
| 1 | `keybindings-help` | `skills/bundled/keybindings.ts` | Use when the user wants to customize keyboard shortcuts, rebind keys, add chord bindings, or modify ~/.mossen/keybindings.json. Examples: |

