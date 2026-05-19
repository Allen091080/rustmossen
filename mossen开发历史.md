# Mossen 开发历史、版本锚点与回滚手册

> 本文档放在主仓根目录，作为后续 Allen 和任何 AI 接手 Mossen 时的长期索引。
> 它记录过去这轮升级做过什么、为什么做、对应 tag/commit 是什么、出了问题怎么查、怎么回滚、还有哪些待办。
>
> 最后更新：2026-04-30  
> 当前主仓：`/Users/allen/Documents/aiproject/mossensrc`  
> 当前主仓 HEAD：`200b2a1cbdc4db0cc856537b8a6020306dd2043a`  
> 当前主仓 tag：`wave5-complete-20260429`  
> Wave6/Wave7 工作树：`/Users/allen/Documents/aiproject/mossensrc-wave5`，当前 HEAD `8a1377d`，尚未合并 main。  
> Workbench 仓：`/Users/allen/Documents/aiproject/mossen-workbench`，当前 HEAD `0c2f5f5`。

---

## 0. 这份文档的用途

这份文档回答四个问题：

1. **我们做过什么**：从早期源码精读、品牌审计、OTel/GrowthBook 清理，到 Wave1-Wave7、Workbench Phase 0。
2. **每个版本锚点是什么**：有哪些 tag，分别代表什么阶段，出问题时可以查哪一个点。
3. **出了问题怎么回滚**：推荐用 revert commit，不默认使用 reset/force。
4. **后续 AI 怎么接手**：哪些红线不能碰，哪些技术债还要继续清理。

这不是普通 release note，而是“开发过程 + 设计决策 + 回滚索引 + AI 接手说明”。

---

## 1. 当前仓库关系

### 1.1 主仓

- 路径：`/Users/allen/Documents/aiproject/mossensrc`
- 分支：`main`
- 当前 HEAD：`200b2a1`
- 当前 tag：`wave5-complete-20260429`
- 状态：Wave5 已完成并推送 origin/github；Wave6/Wave7 尚未合入。

### 1.2 Wave 工作树

- 路径：`/Users/allen/Documents/aiproject/mossensrc-wave5`
- 分支：`worktree/wave5`
- 当前 HEAD：`8a1377d`
- 用途：承载 Wave6/Wave7 未合并改动。
- 当前重要 commit：
  - `bbdcba6` Wave6 leaf USER_TYPE helper
  - `f919ed0` Wave6 收口 commit
  - `8a1377d` Wave7 Door Lock 文档化

### 1.3 Workbench 仓

- 路径：`/Users/allen/Documents/aiproject/mossen-workbench`
- 当前 HEAD：`0c2f5f5`
- 用途：Mossen 桌面 Workbench / Tauri Phase 0。
- 当前已完成：
  - W0 baseline Tauri shell
  - W1 prune template
  - W2 adapter boundary
  - W3a Tauri subprocess bridge
  - W3b loopback Tauri fake bridge smoke

---

## 2. 永久红线

这些约束贯穿整个升级过程，后续 AI 必须继续遵守：

- 不擅自 `push`、`tag`、`merge main`、`rebase`、`reset`、`stash`、`force push`。
- 不用 `git add .` / `git add -A`，除非 Allen 明确允许。
- 不通过修改 harness/smoke 判定逻辑让测试变绿。
- 不破坏 case 39 fingerprint：`870f99ed494d3d145ed2eb1368132299`。
- 不触碰 `commands/insights.ts`，除非 Allen 明确解除 WIP 保护。
- 不乱改 i18n hardcoded allowlist；如果行号变化必须先汇报，等待 Allen 拍板。
- 不把 Workbench 与 mossensrc 的改动混在一个 commit。
- 不删除 worktree，除非 Allen 明确要求。
- 遇到 boot-cycle、DCE、top-level require、权限模式、harness 冲突，先 STOP 汇报。

---

## 3. 高层时间线

### 2026-04-18 至 2026-04-23：项目启动、仓外调研与方向定型

这一段是 Mossen 项目的真实启动期。需要特别说明：**主仓 Git 最早 commit 是 2026-04-24 的 `fed6380 Initial Mossen source import`**，所以 4/18-4/23 不是通过主仓 commit 追踪，而是通过 Allen 的项目记忆和后续文档体系反推的“仓外准备期”。这段不能省略，因为后面所有工程决策都来自这里。

这一阶段的核心不是改代码，而是确定“要做什么”和“不能做什么”：

- 确定 Mossen 不是简单换壳，而是一个面向个人开发者的 CLI-first AI 编程工具。
- 明确第一阶段以 **CLI 稳定内核** 为主，不先做 Web、Mobile、Workbench、RepairPipeline。
- 明确 Mossen 要保留 agent loop、上下文、memory、skill、MCP、tools、permissions 等核心能力，而不是只做一个轻量聊天壳。
- 明确不能让用户看到 Claude / Anthropic / 内部模型名 / 内部平台路径等残留。
- 明确后续所有 AI 开发必须有边界、施工包、验证矩阵和回滚点，不能靠“感觉改一批”。

后续在 Desktop 中沉淀出来的文档体系，就是这段仓外思考的落地：

| 文档/目录 | 后续落地时间 | 作用 |
| --- | --- | --- |
| `mossen升级/README.md` | 4/27 落盘 | Mossen 升级方案总入口 |
| `01-边界与约束/Mossen稳定内核边界.md` | 4/27 落盘 | 定义哪些是 CLI 稳定内核，哪些不能随便动 |
| `01-边界与约束/Mossen源码边界地图.md` | 4/27 落盘 | 后续源码精读和 Wave 拆分依据 |
| `02-扩展系统协议/Mossen扩展系统协议.md` | 4/27 落盘 | plugin / skill / MCP / workflow 等扩展边界 |
| `03-自进化治理/Mossen自进化治理模型.md` | 4/27 落盘 | 自修复、治理、隐私和闭源保护原则 |
| `04-升级路线图/Mossen升级路线图与功能使用说明.md` | 4/27 落盘 | CLI-first 路线图 |
| `05-功能施工包/Stage1-CLI基线加固.md` | 4/28 落盘 | Stage1 的具体施工与验收口径 |

这段历史的一个重要教训是：**项目启动时已经决定 Mossen 要长期维护，而不是一次性 patch**。因此后面才会持续建立 tag、harness、baseline、architecture docs、Door Lock、Workbench 边界和回滚手册。

如果以后要继续补 4/18-4/23 的更细记录，需要 Allen 从聊天记录、外部草稿或当时本地文件中补充原始证据；主仓 Git 本身无法还原这几天的细节。

### 2026-04-24：源码正式入仓、Phase 0 基础设施与 bridge 清理

4/24 是主仓可追踪历史的起点。`fed6380` 把 Mossen 源码正式导入，并打上 `pre-upgrade-baseline-20260424`。这一天主要做三类事：建立项目规划、建立基础工程护栏、清理早期 bridge/remote 残留。

关键 tag：

| tag | 作用 |
| --- | --- |
| `pre-upgrade-baseline-20260424` | 源码正式入仓基线 |
| `pre-rollback-20260424-1756` | 早期回滚保护点 |

代表性 commit：

- `fed6380`：Initial Mossen source import。
- `3d03315`：新增 `MOSSEN.md`、升级路线图、Phase 0 todos。
- `d6887a1`：新增 `CONTRIBUTING.md`，确立 slice-based workflow 与 sandbox rules。
- `0a74000` / `ad801c6` / `56122e2`：加载 `~/.mossen/custom-backend.env`、placeholder key 友好提示、env example。
- `8c8eac1` / `83d54c8` / `03ae3d5`：TypeScript / ESLint / bun install 基础设施。
- `496052b` / `f8b9b69`：statusline context 语义审计与真实 API usage 优先。
- `b50f111`：自动生成 command inventory。
- `d8ed3c5` 到 `14d7ac2`：bridge/remote/ultraplan/CCR 相关残留分 7 个 slice 清理，随后修 TUI startup crash。

这一阶段形成的工程习惯：

- 所有大改必须先建安全锚。
- 每个 slice 要能独立验证。
- 不把 bridge / remote / CCR 这类上游内部形态留在个人版主链路里。
- 先建立可运行、可检查、可回滚的 CLI 基线，再谈体验升级。

### 2026-04-25：核心稳定性、ErrorBoundary、memory、harness P0/P1 与品牌视觉

4/25 是“把 Mossen 从能跑变成可验证”的关键一天。大量工作围绕 typecheck/lint baseline、错误边界、memory、tool wrapper、真实 e2e harness 展开。

主要成果：

- 建立 `tsconfig` / `typecheck:diff` 基线与 lint baseline，冻结既有错误，后续只允许 0 NEW。
- 引入 MossenErrorBoundary，并逐步包住高风险 UI 组件。
- 修复早期 harness 假阳性：tool wrapper、ErrorBoundary、memory loader 都要求真链路。
- 建立 P0/P1 harness 和多层 e2e 验证框架。
- 完成 Mossen 绿叶 logo 替换与 welcome 视觉基础。
- 清理 token-count / VCR / hosted countTokens 的旧路径。

代表性 commit：

- `49564dc` / `06a8bb7` / `7f8298d`：typecheck/lint 基线与脚本。
- `3fe8696` / `02404af` / `7bd589b`：MossenErrorBoundary 与高风险组件包裹。
- `878bd2d`：修 harness，真测 Mossen tool wrapper，不再测 Node 原语。
- `e97d365` / `f513e28`：cross-window memory 真链路验证。
- `87f86e2`：建立 e2e 测试基础设施 + 14 个真链路 smoke。
- `19cf8fe` / `24660ee` / `2438d02` / `cd2d66d` / `6cf5a60`：memory / skill / plugin / manual compact P1 e2e。
- `b1c9560` / `bac0694` / `22c6e76`：Clawd 橙色吉祥物替换为 Mossen 绿叶 logo，并调整尺寸。
- `8ddf54e` / `39b65d7` / `86e5e92`：清理 anthropic-hosted countTokens、增强 counttokens/MCP truncation smoke、移除 dead VCR helpers。

这一阶段的教训：

- 只读 shape 检查不够，必须有 mutation evidence 或真实链路。
- memory / tool / error boundary 这类核心能力要用真实 Mossen 路径测，不用 raw fs 或 Node 原语冒充。
- 视觉替换不是单纯改 logo，还要防止 Welcome fallback、statusline、组件边界回退到旧品牌。

### 2026-04-26：附录 C、核心能力矩阵与 custom backend 回归

4/26 主要是把 P0/P1 后的验证补全成更完整的核心能力矩阵。到这一天，harness 从“能覆盖主路径”扩展到 memory、skill、MCP/plugin、权限、命令、model/profile、long task、nested subtask 等大量真实能力。

主要成果：

- 附录 C 大量 e2e 补齐，最终记录为 58/58。
- 权限进阶 smoke 覆盖 permission modes / edit write permission / permission scope。
- skill 进阶 smoke 覆盖 skill list / invoke / reload / inject。
- memory 进阶 smoke 覆盖 project memory、新窗口、resume 边界。
- 命令 inventory 与 slash command 体系得到大范围验证。
- custom backend 下 auto-compact 真触发，取消 skipped。
- print mode 加 SIGTERM gracefulShutdown，保证 session log flush。
- harness 加 retry，减少 LLM transient 导致 gate 假挂。

代表性 commit：

- `0960765`：M2.4/M2.5/M2.6 权限进阶 e2e。
- `606224a`：M3.4/M3.5/M7.3/M7.4 MCP/plugin 隔离 e2e。
- `2ce626d`：附录 C 第二波 15 个 e2e。
- `8d1ecbf`：M8.1-M8.4 101 slash command 全量验证。
- `5bd5b61`：M13.1 + M13.2 总 gate。
- `cb95e57`：附录 C 全完成 22/22，总 58/58 收尾。
- `410a594`：auto-compact 在 custom backend qwen 真触发。
- `3e4338b`：print mode 加 SIGTERM → gracefulShutdown。
- `11be722`：多 case 主流程加 retry，降低 transient 失败。

这一阶段奠定了后续 Wave 的底气：不是只看 grep，而是有一套能证明 CLI core、agent loop、memory、skill、permissions、model/profile 不退化的 harness 基础。

### 2026-04-27：OTel 移除与 GrowthBook 迁移

这一阶段处理历史 OTel 与 GrowthBook 迁移工作，减少不必要依赖与远端配置面。

关键 tag：

| tag | 作用 |
| --- | --- |
| `pre-otel-phase-X` / `Y` / `A` / `B` / `C` / `D` / `E` / `F` | OTel 移除阶段保护点 |
| `post-otel-removal-20260427` | OTel 移除完成点 |
| `pre-growthbook-migration-20260427` | GrowthBook 迁移前锚点 |
| `pre-growthbook-G1` ~ `pre-growthbook-G6` | GrowthBook 分阶段保护点 |
| `post-growthbook-migration-20260428` | GrowthBook 迁移完成点 |

代表性 commit：

- `9fc52b8`：`bun remove @growthbook/growthbook`
- `946fc0a`：GrowthBook dependency removal 验收同步

### 2026-04-28：Stage 1 CLI 基线、多 profile、custom backend

这一阶段把 Mossen CLI 的基础能力稳定下来，尤其是 custom backend、多 profile、模型切换、认证 header 等。

关键 tag：

| tag | 作用 |
| --- | --- |
| `post-stage1-cli-baseline-20260428` | Stage 1 总验收，harness gate 通过 |
| `stable-cli-v1.0-20260428` | CLI v1.0 稳定点 |

代表性 commit：

- `db953ae`：multi-profile schema + facade
- `925943d`：customBackend profile-aware getter
- `8fa0dbc`：7 个 multi-profile CLI flag
- `0c84acb`：`/model` 接 multi-profile schema
- `cd4e5f4`：OpenAI-compatible custom backend 使用 `Authorization: Bearer`

### 2026-04-28：品牌清理 Wave1 / Wave1.5

目标是清理用户可见或高风险的旧品牌残留，包括 Claude/Anthropic/内部模型名/死链/死分支。

关键 tag：

| tag | 作用 |
| --- | --- |
| `wave1-brand-cleanup-20260428` | Wave1 品牌清理完成 |
| `wave1.5-needs-design-low-risk-20260428` | needs-design 低风险注释收口 |

代表性 commit：

- `7331f92`：MCP server name `mossen/tengu` → `mossen`
- `3e00e01`：通知文案中文化
- `d92fb58`：WelcomeV2 删除 ASCII Clawd fallback
- `5305867` / `d112666`：`Mossen` 文案改为产品名 helper
- `2fa03f4` / `1d0950e`：移除 opus/内部模型名硬编码残留

### 2026-04-28：UX / i18n 基础

目标是建立中文/英文文案机制、命令 description 字典、硬编码 user text 审计 baseline。

关键 tag：

| tag | 作用 |
| --- | --- |
| `pre-ux-wave1-20260428` | UX/i18n 前锚点 |

代表性 commit：

- `313e459`：i18n 字典骨架 + self_check
- `943b662`：i18n 硬编码 baseline 扫描脚本
- `2674be8`：中文 spinner 词库扩展
- `a45c76e`：builtin command description 落字典
- `ffcf168`：`/lang` 文案 i18n
- `871353a`：i18n runtime smoke 脚本

### 2026-04-28：Wave0 权限与 retry 基础修复

目标是先修高危权限与 API retry 风暴问题。

关键 tag：

| tag | 作用 |
| --- | --- |
| `pre-s3-action-wave0-20260428` | Wave0 前锚点 |
| `wave0-perm-retry-20260428` | 权限/retry 修复完成 |

代表性 commit：

- `cbcf467`：网络/云写检测对所有用户启用
- `97c923e`：overly broad 权限检测对所有用户启用
- `3cff73a`：withRetry 用 `getUserType()` 修 529 retry storm
- `d5cc966`：USER_TYPE lock 初版设计记录

### 2026-04-28 至 2026-04-29：Wave2 潜伏激活面治理

目标是清掉隐藏激活面、死代码、危险 env strip、远端/内部路径。

关键 tag：

| tag | 作用 |
| --- | --- |
| `pre-s4-wave2-20260428-2204` | Wave2 前总锚点 |
| `pre-wave2-A1-bashbq-20260428-2218` | Bash BQ 处理前 |
| `pre-wave2-A2-envstrip-20260428-224119` | env strip 收缩前 |
| `pre-wave2-A5-cmdallowlist-20260428-224437` | command allowlist 前 |
| `pre-wave2-A3-ccr-20260428-230608` | CCR remote isolation 前 |
| `pre-wave2-A4-canonical-20260428-231237` | canonical skill 前 |
| `pre-wave2-A6-stuck-20260428-232209` | stuck skill 前 |
| `pre-wave2-A7-growthbook-20260428-232557` | GrowthBook prompt KV 前 |
| `pre-wave2-BC2-dumpprompts-20260428-233212` | dumpPrompts 前 |
| `pre-wave2-C1-typemig-20260428-234013` | promptSuggestion USER_TYPE 前 |
| `wave2-complete-20260428-234434` | Wave2 完成 |
| `wave2-user-type-cleanup-20260429` | Wave2 USER_TYPE cleanup |

代表性 commit：

- `3a167f0`：`logClassifierResultForMossen` 函数体置空，保留签名
- `e084441`：`MOSSEN_ONLY_SAFE_ENV_VARS` 30 → 6
- `4bc6d22`：`gh` read-only allowlist 公共化，移除敏感命令
- `ac38d54`：删除 CCR `isolation:'remote'`
- `5e50bdf`：删除 canonical skill 远程路径
- `e7fab78`：`getAntModelOverrideSection` return null，杜绝远端 KV 注入 prompt
- `9a97d51`：删除 `/stuck` skill
- `7ecf0e8`：删除 dumpPrompts.ts 与引用链
- `62231b7`：promptSuggestion 单点 USER_TYPE 收敛

### 2026-04-29：UX-Wave2 命令 description i18n

目标是完成高频命令 description 的中英双语字典化。

关键 tag：

| tag | 作用 |
| --- | --- |
| `wave2-i18n-cmd-desc-20260429` | UX-Wave2 command description 完成 |

代表性 commit：

- `f67e098`：高频会话 10 个命令
- `b2f4a64`：编辑/配置 8 个命令
- `2fda3a9`：PR/security/login/advisor
- `8ec007c`：Plugin/Skill/IDE
- `ab71e89`：`/btw`
- `6d8d531`：合并 28 个 command keys

### 2026-04-29：Wave3 安全清理与命名收口

目标是继续去掉 remote/dead branch/旧内部命名/Slack/Anthropic 注释残留。

关键 tag：

| tag | 作用 |
| --- | --- |
| `wave3-batch123-r4-cleanup-20260429` | Wave3 batch + R4 命名清理完成 |

代表性 commit：

- `6ca3161`：中和 Slack/anthropic 注释
- `eb8bcff`：删除 `remote_launched` 与 `external === mossen` 死分支
- `dfff583`：中和 5 个用户可见 `[ANT-ONLY]`
- `3ee02ad`：删除 experimental skill search dead require
- `da4f590`：清理 dumpPrompts ghost chain
- `16bdb1d`：`isAnt` → `isInternal`
- `8d495f9`：prompt 命名清理
- `5620578`：删除 deprecated ant model aliases

### 2026-04-29：Wave4 架构治理

目标是建立长期架构边界、统一 smoke runner、feature flag 审计、CWB/stream-json guardrails。

关键 tag：

| tag | 作用 |
| --- | --- |
| `wave4-architecture-governance-stage12-20260429` | Wave4 stage 1/2 完成 |

代表性 commit：

- `5497361`：architecture knowledge base / red-lines / audit checklist
- `8ac5e41`：`run_all_smoke.sh` 统一 smoke runner
- `9329460`：feature flag audit smoke
- `e586296`：Core CLI Workbench layer guardrails
- `945609a`：stream-json contract guardrails

### 2026-04-29：Wave5 已合并主仓

Wave5 是当前主仓的最新完成点。目标是删除 BRIDGE_MODE debt、收敛 R2/R3 USER_TYPE 风险点。

关键 tag：

| tag | 作用 |
| --- | --- |
| `wave5-complete-20260429` | Wave5 完成，当前 main HEAD |

Wave5 主要阶段：

| Phase | commit | 内容 |
| --- | --- | --- |
| Phase 1 | `25b452b` / merge `10baf6f` | 删除 `discoveredSkillNames` dead field |
| Phase 2 | `6cd78c9` / `b85911e` / `98f0520` / merge `0a4481b` | 删除 BRIDGE_MODE feature flag debt + env example |
| Phase 3 | `4ee40f1` / merge `32721f1` | yoloClassifier 6 处 USER_TYPE runtime gate 收敛，line 72 DCE inline 保留 |
| Phase 4 | `ec729c2` / merge `a3e7f75` | firstPartyEventExporter 12 处诊断 gate 收敛 |
| Phase 5 | `fe418d4` / merge `85c594f` | firstPartyEventLogger 7 处诊断 gate 收敛 |
| Phase 6 | `d65d6ef` / merge `ec85669` | withRetry 2 处 USER_TYPE 收敛 |
| Phase 7 | `5c35be4` / merge `0344b09` | mockRateLimits 11 处用 module-local helper，避免 sessionStorage import cycle |
| Phase 8 | `0c811d1` / merge `200b2a1` | mossen API 8 处 USER_TYPE 收敛 |

Wave5 的重要教训：

- `services/mockRateLimits.ts` 不能直接 import `sessionStorage`，会触发 `utils/auth → mockRateLimits → sessionStorage → tools` 的 TDZ cycle。
- boot-cycle 风险不是静态等价证明能覆盖的，后续凡是新增 `sessionStorage`/重依赖 import，必须跑 `bun run help` boot smoke。
- line 72 yoloClassifier DCE inline 保留，不要随意替换。

---

## 4. Wave6：尚未合并主仓

Wave6 在 `mossensrc-wave5` 工作树中完成，尚未合并 main。目标是建立 zero-dependency `utils/userType.ts` leaf helper，并收敛一批低/中风险 USER_TYPE gates。

Wave6 commit 链：

| commit | 内容 |
| --- | --- |
| `bbdcba6` | 抽出 `utils/userType.ts` leaf helper，迁移 6 个 caller，并同步 2 个已授权 smoke 字面量断言 |
| `59f0fb4` | W6-1 `constants/prompts.ts` 7 处 runtime gate，undercover DCE 共生保留 |
| `41b9eb6` | W6-2 permissions diagnostic gates |
| `bf7a407` | W6-3 `shouldUseSandbox` gate |
| `038128d` | W6-4 `bashPermissions` 4 处 gate |
| `bb7cadc` | W6-5 permissions runtime gates |
| `9e95a2b` | W6-6 commands runtime gates，`context-noninteractive.ts` 使用 module-local helper 避免 i18n allowlist 行号漂移 |
| `fad637a` | W6-7 cost / feedback / init gates，部分文件使用 module-local helper 保持 allowlist 行号 |
| `f919ed0` | W6-8 cli / bootstrap / migration gates |

Wave6 重要决策：

- `utils/userType.ts` 必须保持 zero-dependency leaf module。
- `sessionStorage.ts` re-export `getUserType` 保持兼容。
- 某些 i18n allowlist 文件不能新增顶部 import，否则行号变化；采用 module-local helper 是临时折中。
- `dangerousPatterns.ts` / `classifierDecision.ts` / `yoloClassifier.ts` 等 DCE/inline 点按审计结论保留。
- Wave6 收口复核显示 run_all、typecheck、lint、case39 均通过，但尚未进入 main merge gate。

---

## 5. Wave7：尚未合并主仓

Wave7 目标是处理剩余高价值 USER_TYPE 风险点，并最终改为 Door Lock 思路：公开版入口统一锁住 `USER_TYPE=ant/mossen`，避免继续逐点清理所有灰区。

Wave7 commit 链：

| commit | 内容 |
| --- | --- |
| `f97c0a9` | W7-B `commands.ts` internal command spread gate |
| `10b11a9` | W7-C commit / commit-push-pr gates，使用 module-local helper 避免 allowlist 行号漂移 |
| `4f4582d` | W7-D attribution gates |
| `5742ac6` | W7-E 删除 unused `query/config.ts` internal gate 字段 |
| `e5c30ce` | W7-A issue flag banner gate |
| `8fd3bb6` | JS 层 Door Lock：`utils/userTypeRuntimeLock.ts` + `getUserType()` normalize |
| `def26fd` | Shell 层 Door Lock：`run-bun-featured.sh` 在真实 CLI 入口启动 Bun 前归一化 USER_TYPE |
| `8a1377d` | 记录 Door Lock 设计债与后续清理 TODO |

### 5.1 Door Lock 的原因

`USER_TYPE=ant/mossen` 是内部/预留运行模式开关，不是普通用户身份。它控制：

- ant-only top-level require，例如 `tools.ts` 的 `REPLTool` / `SuggestBackgroundPRTool`
- `INTERNAL_ONLY_COMMANDS`
- `[MOSSEN INTERNAL]` 诊断输出
- undercover/internal 防泄漏路径
- mossen-only 权限与 prompt 行为

旧方案只在 `entrypoints/cli.tsx` 调 `applyUserTypeRuntimeLock()`。实测失败：

```bash
USER_TYPE=ant bun run help
```

会在 ESM import hoisting 阶段先评估 `tools.ts`，触发 ant-only require，尝试加载公开版不存在的 `REPLTool`，导致启动失败。

修复后：

- `run-bun-featured.sh` 只在真实 CLI 入口 `entrypoints/cli.tsx` 时执行 shell lock。
- `bun -e` / `--eval` 不 shell lock，保留测试 raw USER_TYPE 的能力。
- `utils/userTypeRuntimeLock.ts` / `utils/userType.ts` 作为 JS 兜底。
- `MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE=1` 是显式 unlock 口。

### 5.2 Door Lock 当前规则

| 输入 USER_TYPE | unlock | 输出 |
| --- | --- | --- |
| unset / empty | 任意 | `external` |
| `external` | 任意 | `external` |
| `ant` | unset / 非 `1` | `external` |
| `mossen` | unset / 非 `1` | `external` |
| unknown | 任意 | `external` |
| `ant` | `MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE=1` | `ant` |
| `mossen` | `MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE=1` | `mossen` |

### 5.3 Wave7 未做/后续事项

- `USER_TYPE === 'mossen'` 19 处 / 14 文件是预留模式体系，需 Wave8+ 整体审计，不要零散修改。
- `query.ts:927 stripSignatureBlocks` 暂未做；是否需要取决于 Door Lock 后是否还值得继续逐点清理。
- 剩余 boot-time/top-level/DCE inline 需要单独审计，不要批量乱改。
- module-local helper 是为了 i18n allowlist 行号稳定的折中，未来 allowlist 机制升级后可统一回 `utils/userType.ts`。

---

## 6. Workbench Phase 0

Workbench 是独立仓，不在 mossensrc 主仓中。它用于把 Mossen CLI 能力接入 Tauri 桌面 Workbench。

路径：`/Users/allen/Documents/aiproject/mossen-workbench`

commit 链：

| commit | 内容 |
| --- | --- |
| `bca4a7f` | W0 baseline Tauri workbench template |
| `0cd41f2` | W1 prune template to Phase 0 shell |
| `4176a3f` | W2 minimal Mossen binary adapter boundary |
| `ee4cb34` | W3a Tauri subprocess bridge for stream-json |
| `0c2f5f5` | W3b Tauri loopback fake bridge smoke |

Workbench 当前结论：

- W3a 走自定义 Tauri command，不引入 `tauri-plugin-shell`。
- W3a fake-only 子进程桥已通过 `smoke:bridge`。
- W3b 证实 macOS WKWebView 的 `console.log` 不稳定，改走 loopback HTTP 回报通道。
- W3b 证实自定义 command 在 minimal capability 下可由 webview invoke。
- W3b 仍是 fake-only，不接真 Mossen，不消耗真实 LLM。
- 后续真 mossen 接入建议在 mossensrc Wave 合并稳定后再做。

---

## 7. 已知报告位置

桌面报告根目录：

```text
/Users/allen/Desktop/mossen升级/07-源码精读与品牌审计/审计结果/
```

重要子目录：

| 路径 | 内容 |
| --- | --- |
| `wave2-prep/` | Wave2 调研/施工报告 |
| `wave3-prep/` | Wave3 调研/施工报告 |
| `wave4-prep/` | Wave4 架构治理报告 |
| `wave5-prep/` | Wave5 启动前复核、执行记录、收口报告 |
| `wave6-prep/` | Wave6 Phase0、多 SA 审计、收口复核 |
| `wave7-prep/` | Wave7 Phase0、Door Lock、W7-H 调研 |
| `final-branch-validation/` | 早期最终验证尝试，曾因 runner 兼容问题中断 |
| `final-branch-validation-20260430/` | 最新最终验证日志目录 |

不要把 Desktop 报告当源码改动提交，除非 Allen 明确要求。

---

## 7.1 桌面 `mossen升级` 目录是项目档案库

除了主仓 commit 历史，桌面目录 `/Users/allen/Desktop/mossen升级` 是这次项目的“方案库 / 审计库 / 施工包库”。很多 4/18-4/23 的启动期思考、4/27 之前未入仓的方向判断、以及后续每个 Wave 的只读报告，都不在 Git commit 里，只在这个目录里。

后续 AI 如果只看 git log，会误以为项目从 4/24 或 4/27 才开始；这是错误的。正确做法是：

1. 先读本文件，理解总时间线。
2. 再读 `/Users/allen/Desktop/mossen升级/README.md`，理解方案库结构。
3. 遇到边界问题，查 `01-边界与约束/`。
4. 遇到扩展、Workbench、Repair、CLI 体验问题，查对应目录。
5. 遇到 Wave 决策与审计细节，查 `07-源码精读与品牌审计/审计结果/`。

### 7.1.1 顶层目录索引

| 目录 | 作用 | 什么时候查 |
| --- | --- | --- |
| `01-边界与约束/` | 稳定内核边界、源码边界地图、Core CLI / Workbench 分层、CWB 边界 | 任何涉及核心架构、Workbench、stream-json、CLI binary packaging 的改动前 |
| `02-扩展系统协议/` | skill/plugin/workflow/prompt/language/slash/hook/panel/MCP 扩展协议 | 后续要做扩展系统、插件市场、skill pack、hook pack 时 |
| `03-自进化治理/` | 本地进化、产品进化、核心修复、诊断包、审计、回滚治理 | 后续要做自修复、RepairPipeline、自动建议、能力沉淀时 |
| `04-升级路线图/` | CLI-first 路线图、功能优先级、暂缓能力 | 决定下一阶段做什么、不做什么时 |
| `05-功能施工包/` | Stage1-5 的施工包、AI 执行提示词、自审报告 | 需要把设计变成可执行任务时 |
| `06-上游跟进机制/` | Claude Code 公开变化跟进机制 | 后续要吸收上游公开产品经验但不追源码时 |
| `07-源码精读与品牌审计/` | 全源码精读、品牌残留审计、Wave0-Wave7 审计和执行报告 | 查 Claude/Anthropic 残留、USER_TYPE、Wave 决策、架构卡片时 |
| `08-体验升级/` | CLI/TUI 体验升级、i18n、实机复核、UX Wave | 做终端体验、中文化、欢迎屏、命令文案时 |

### 7.1.2 必读根文档

| 文件 | 作用 |
| --- | --- |
| `README.md` | 桌面方案库总入口，定义阅读顺序、什么时候能动代码、什么时候必须停下问 Allen |
| `ai完整施工包的定义.md` | 规定“完整施工包”标准：目标边界、只读审计、用户决策、slice、并发纪律、测试、回滚、执行提示词 |
| `01-边界与约束/Mossen稳定内核边界.md` | 定义 Core 范围：启动、输入、agent loop、模型、tool executor、权限、context、memory、resume、MCP、skill、hooks、stream-json、CLI 渲染、harness |
| `01-边界与约束/Mossen源码边界地图.md` | 把源码目录映射到 Core / Extension / Surface / Policy / Evolution / Optional 层 |
| `04-升级路线图/Mossen升级路线图与功能使用说明.md` | 定义 CLI-first 策略与功能优先级 |
| `05-功能施工包/Stage1-CLI基线加固.md` | Stage1 的基线加固、harness、custom backend、profile、验证体系 |
| `07-源码精读与品牌审计/Mossen全源码精读与品牌残留审计施工包.md` | 全源码精读和品牌残留审计的总施工包 |
| `07-源码精读与品牌审计/ai全源码精读与品牌残留审计执行提示词.md` | 给 AI 执行源码精读的原始提示词 |

### 7.1.3 全源码精读与品牌审计产物

`07-源码精读与品牌审计/审计结果/` 是最重要的证据目录。它不是临时文件，而是后续每个 Wave 的依据。

| 子目录/文件 | 内容 |
| --- | --- |
| `00-manifest/` | baseline 与 dispatch plan，说明审计如何分派 |
| `01-keyword-scan/` | Claude / Anthropic / tengu / GrowthBook / hosted / OAuth / model names 等关键词扫描结果 |
| `02-module-cards/` | 启动入口、core agent loop、utils、components、commands、services、tools、hooks、runtime、extension/test/docs 的模块卡片 |
| `04-module-architecture/` | 架构知识库：普通人故事层、产品逻辑层、技术架构层、设计思想、AI施工层 |
| `05-brand-residuals/` | 品牌残留分类：must-fix、needs-design、allowed-and-saved |
| `06-naming-domain-audit/` | 命名域审计：public surface、协议边界、rename candidates、state/storage migration、release/package map |
| `07-quality-gates/` | final audit summary 与 cross-review checklist |
| `S2-research/` | USER_TYPE / skill / verify / remember / stuck 等专项研究 |
| `USER_TYPE-runtime-gate-research/` | USER_TYPE runtime gate 风险审计与入口分析 |
| `wave2-prep/` 到 `wave7-prep/` | 每个 Wave 的只读复核、SA 报告、施工包、收口报告 |

后续如果要判断“某个 Claude/Anthropic 字符串是否已经审过”，不要只 grep 当前源码；要同时查：

- `01-keyword-scan/`
- `05-brand-residuals/`
- `06-naming-domain-audit/`
- 对应 Wave 的 prep 报告

### 7.1.4 Workbench / CWB 档案

Workbench 的边界不是临时讨论出来的，桌面 `01-边界与约束/` 下有完整 CWB 档案：

| 文件 | 作用 |
| --- | --- |
| `Mossen-Core-CLI-Workbench分层边界施工包.md` | Core CLI 与 Workbench 的分层边界 |
| `CWB-3-Protocol-Export-Hardening-只读审计.md` | stream-json / protocol export hardening 只读审计 |
| `CWB-3-Protocol-Export-Hardening-施工包草案.md` | 协议契约与静态防护 smoke 草案 |
| `CWB-4-Workbench-Adapter-最小开工施工包.md` | Workbench adapter Phase 0 最小开工方案 |
| `CWB-5-CLI-Binary-Packaging-边界施工包.md` | CLI binary packaging 边界，防止把 Workbench/Web/Mobile 打进 CLI |
| `Workbench-W3a-Subprocess-Bridge-启动前复核.md` | W3a subprocess bridge 路线选择，推荐自定义 Tauri command |

Workbench 的长期原则：

- 第一阶段独立 repo。
- 通过 `mossen` 二进制 subprocess + stream-json 接入 Core。
- 不 in-process import `mossensrc`。
- Workbench 不打包 mossen CLI binary。
- Workbench 的 smoke 不应污染 Core CLI harness。

### 7.1.5 体验升级档案

`08-体验升级/` 记录了 CLI/TUI 体验升级的设计与暂停点：

| 文件 | 作用 |
| --- | --- |
| `Mossen-CLI体验升级总施工包.md` / `CLI体验升级总施工包.md` | CLI 体验升级总方向 |
| `UX-Wave1-语言底座施工包.md` | 中文/英文语言底座、命令 description、spinner、interrupt 文案 |
| `W2-command-description-施工包.md` | 后续命令 description i18n 施工包 |
| `UX-TUI-体验升级-施工包.md` | TUI 面板化设想 |
| `UX-TUI-终端界面升级暂停说明.md` | 为什么默认 CLI 仍保持传统 scrollback，不贸然上固定顶栏/右栏 |
| `UX-TUI-1-实机复核报告.md` | 人眼实机复核记录 |

重要结论：

- Mossen 的默认体验仍是 Classic CLI，不为了视觉升级牺牲稳定性。
- 右栏、面板、Workbench-like 信息架构要等边界清楚后做。
- 语言底座和命令 description 可以逐步做，但必须过 i18n hardcoded audit。

### 7.1.6 桌面报告与主仓文档的关系

主仓内的 `mossen开发历史.md` 是索引和长期手册，不替代 Desktop 里的详细报告。关系如下：

- **主仓文档**：适合未来 AI 快速了解历史、tag、回滚、红线、下一步。
- **Desktop 报告**：适合追溯某个决策的证据、只读复核、候选方案、失败原因、Allen 拍板过程。
- **Git commit**：适合确认代码实际做了什么。

三者缺一不可。未来查问题时应按这个顺序：

1. 看 `mossen开发历史.md` 定位阶段。
2. 看对应 tag/commit 确认代码状态。
3. 看 Desktop 对应报告理解为什么这么做。
4. 再决定修复、revert、继续施工或暂停。

---

## 8. Tag 索引

### 8.1 当前最重要 tag

| tag | commit/位置 | 用途 |
| --- | --- | --- |
| `stable-cli-v1.0-20260428` | CLI v1.0 稳定点 | custom backend / profile 修复后稳定基线 |
| `wave0-perm-retry-20260428` | Wave0 | 权限与 retry storm 修复 |
| `wave1-brand-cleanup-20260428` | Wave1 | 品牌残留清理完成 |
| `wave2-complete-20260428-234434` | Wave2 | 潜伏激活面治理完成 |
| `wave2-i18n-cmd-desc-20260429` | UX Wave2 | 命令 description i18n 完成 |
| `wave3-batch123-r4-cleanup-20260429` | Wave3 | 安全清理与命名收口 |
| `wave4-architecture-governance-stage12-20260429` | Wave4 | 架构治理与 smoke guardrails |
| `wave5-complete-20260429` | `200b2a1` | 当前主仓 HEAD，Wave5 完成 |

### 8.2 建议未来 tag

Wave6/Wave7 合并前后建议新增：

| 建议 tag | 指向 | 用途 |
| --- | --- | --- |
| `pre-wave67-merge-20260430` | 合并前 main HEAD `200b2a1` | 合并前安全锚 |
| `wave6-complete-20260430` | Wave6 结束 commit `f919ed0` | Wave6 完成点 |
| `wave7-complete-20260430` | Wave7 当前最终 commit `8a1377d` 或后续验证文档 commit | Wave7 完成点 |
| `wave67-merged-20260430` | 合并后的 main merge commit | Wave6/Wave7 合并落点 |

注意：tag 必须等最终验证 PASS、Allen 拍板后再打，不要提前自动打。

---

## 9. 回滚手册

### 9.1 默认回滚方式：revert merge commit

如果 Wave6/Wave7 合并 main 后出问题，默认不要 `reset`，不要 force push。推荐：

```bash
git revert -m 1 <merge_commit_hash>
```

含义：

- `-m 1` 保留 main 作为主线 parent。
- 生成一个新的 revert commit，把合入分支的改动撤销。
- 历史可追溯，远端安全。

验证：

```bash
bun run help
bun run typecheck:diff
bun run lint:diff
bash scripts/run_all_smoke.sh
```

并确认 case 39 fingerprint：

```text
870f99ed494d3d145ed2eb1368132299
```

### 9.2 如果 revert 有冲突

不要硬解。STOP 并汇报：

- 冲突文件
- 冲突段落
- 当前 HEAD
- 被 revert 的 merge commit
- 是否涉及 harness/smoke/scripts/allowlist

### 9.3 reset 只在 Allen 明确要求时使用

`git reset --hard` / force push 会改写历史。默认禁止。

---

## 10. 合并前最终验证要求

Wave6/Wave7 合并 main 前，至少需要完成：

### 10.1 基础静态验证

- `git diff --check`
- `bun run help`
- `bun run typecheck:diff`
- `bun run lint:diff`
- `python3 scripts/audit_hardcoded_user_text.py`
- `python3 scripts/layer_boundary_audit.py`
- `python3 scripts/stream_json_contract_smoke.py`
- `python3 scripts/wave4_r8_feature_flag_smoke.py`

### 10.2 Door Lock 专项

- `USER_TYPE=ant bun run help` PASS，且不出现 `MOSSEN INTERNAL`
- `USER_TYPE=mossen bun run help` PASS
- `USER_TYPE=weird bun run help` PASS
- `USER_TYPE=ant ./run-bun-featured.sh -e 'console.log(process.env.USER_TYPE)'` 输出 `ant`
- `python3 scripts/error_boundary_usertype_gate_smoke.py` PASS
- `MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE=1` unlock path 行为单独记录

### 10.3 核心能力

- 命令清单与 hidden command smoke
- 权限与 sandbox smoke
- context / memory / skill smoke
- agent loop / long task / nested subtask smoke
- model / profile / custom backend smoke
- `bash scripts/run_all_smoke.sh`
- case39 fingerprint 稳定

---

## 11. 后续 TODO

### 11.1 Wave6/Wave7 merge gate

- 等最终验证 PASS。
- 给 main 打 `pre-wave67-merge-20260430`。
- 给 Wave6/Wave7 完成点打 tag。
- 合并 `worktree/wave5` 到 main。
- 合并后跑最终验证。
- 合并后打 `wave67-merged-20260430`。
- push 由 Allen 明确拍板。

### 11.2 Wave8+ 候选

- `USER_TYPE === 'mossen'` 19 处整体审计。
- 剩余直接 `process.env.USER_TYPE` inline 分类清理。
- top-level / boot-time / DCE require 风险点专项。
- module-local helper 统一化前置：升级 i18n hardcoded allowlist 机制，避免物理行号依赖。
- `query.ts:927 stripSignatureBlocks` 是否仍值得做，需结合 Door Lock 后风险重新判断。

### 11.3 Workbench 后续

- W3b 后可继续完整 adapter 闭环，而不只是 nano-probe。
- 真 mossen 接入建议等 mossensrc Wave6/Wave7 合并稳定后再做。
- W3c 需要单独设计真 mossen binary 白名单与冷启动 smoke。

---

## 12. 给未来 AI 的接手提示

如果你是后续接手的 AI，请先读完本文，再做以下动作：

1. 确认你在正确仓库：主仓是 `mossensrc`，Workbench 是 `mossen-workbench`。
2. 确认当前 HEAD、tag、status。
3. 不要自动 merge/tag/push。
4. 如果用户说“验证”，只跑验证，不改代码。
5. 如果用户说“合并”，先打安全锚 tag，再合并，再验证。
6. 如果用户说“回滚”，默认用 `git revert -m 1`，不要 reset。
7. 任何涉及 harness/smoke/scripts/allowlist 的改动，必须先 STOP 汇报。
8. 任何涉及 USER_TYPE、DCE、top-level require、boot-cycle 的改动，必须先做只读复核。
9. 保持 `case 39` fingerprint 不漂移。

这轮升级的核心思想不是“把所有字符串都替换掉”，而是：

- 用户可见和可触发的风险先清；
- 高风险隐藏激活面先清；
- 公开入口用 Door Lock 锁住内部模式；
- 低价值灰区不盲目追求 0 hit；
- 所有改动必须可验证、可回滚、可解释。
