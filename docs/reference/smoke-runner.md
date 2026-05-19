# run_all_smoke.sh — 统一 smoke runner 使用说明

> **脚本位置**: `scripts/run_all_smoke.sh`
> **目的**: 沉淀 Wave 0~4 各类 audit / smoke 实践, 一条命令跑完所有"快速"验证 (含 Core/CLI/Workbench 边界与 case 39 fingerprint 稳定性).
> **与 `smoke_check.py` 的区别**: 本 runner **不跑** `smoke_check.py` 158 case 全集 (耗时 10+ 分钟); 仅校验 case 39 fingerprint 是否漂移.

---

## 1. 用法

```bash
./scripts/run_all_smoke.sh              # 实跑全部快速验证, 退出码 0/1/2
./scripts/run_all_smoke.sh --dry-run    # 仅列要跑的步骤, 不实际执行 (审计前必先跑)
./scripts/run_all_smoke.sh --help       # 显示帮助
```

## 2. 退出码语义

| 退出码 | 含义 |
|:----:|------|
| **0** | 全部通过 |
| **1** | 任一步骤 fail (列出 fail 的步骤名), case 39 fingerprint 仍稳定 |
| **2** | **case 39 fingerprint 漂移** — 永久红线触发, 立即停下汇报 Allen |

## 3. 覆盖范围 (20 执行单元)

> **口径**: 9 wave2 + 2 wave0_perm 各 glob 展开多个文件; 加上 typecheck/lint/i18n/audit/R8.2/stream-json-contract/layer-boundary/case39 后, 真实执行单元为 20。

| # | 步骤 | 命令 | 来源 |
|:-:|------|------|------|
| 1 | `typecheck:diff` | `bun run typecheck:diff` | TS baseline diff (0 new 校验) |
| 2 | `lint:diff` | `bun run lint:diff` | lint baseline diff (0 new 校验) |
| 3-11 | 9 wave2 focused smoke | `python3 scripts/wave2_*_smoke.py` | Wave 2 9 slice 收口 |
| 12 | wave0_perm1 | `python3 scripts/wave0_perm1_dangerous_patterns_smoke.py` | Wave 0 PERM-1 |
| 13 | wave0_perm2 | `python3 scripts/wave0_perm2_overly_broad_smoke.py` | Wave 0 PERM-2 |
| 14 | i18n_self_check | `python3 scripts/i18n_self_check.py` | UX-Wave 字典对称性 |
| 15 | i18n_runtime_smoke | `python3 scripts/i18n_runtime_smoke.py` | UX-Wave runtime 验证 |
| 16 | audit_hardcoded_user_text | `python3 scripts/audit_hardcoded_user_text.py` | UX-Wave 硬编码扫描 |
| 17 | **wave4_r8_feature_flag_smoke** | `python3 scripts/wave4_r8_feature_flag_smoke.py` | Wave 4 R8.2: bunfig MACRO + feature() 白名单 + resolve 一致性 (Wave 5 Phase 2 后 known debt 清空) |
| 18 | **stream_json_contract_smoke** | `python3 scripts/stream_json_contract_smoke.py` | CWB-3: stream-json 24+21+8+5 schema whitelist + 入口锚点 + only-additive 守卫 |
| 19 | **layer_boundary_audit** | `python3 scripts/layer_boundary_audit.py` | CWB-2: Core / CLI / Workbench / Extension import 边界 |
| 20 | **case 39 fingerprint** | `python3 scripts/smoke_check.py --only custom_backend_auth_runtime_audit` + 提取 JSON + md5 | Wave 2 case 39 永久红线 |

## 4. 不覆盖范围 (按设计跳过)

| 范围 | 跳过理由 | 单跑命令 |
|------|---------|---------|
| `smoke_check.py` 158 case 全集 | 耗时 10+ 分钟, 适合 CI / release 前 | `python3 scripts/smoke_check.py` |
| `harness_M[N]_*` (85 文件) | M 模块 e2e, 单 case 较慢 | `python3 scripts/harness_M[N]_*_smoke.py` |
| `harness_R[N]_*` (9 文件) | R 根因 audit | `python3 scripts/harness_R[N]_*_smoke.py` |
| LLM 真冒烟 | 需手动启动 mossen + 真模型 + manual UX | (手动跑 mossen) |
| 任何 destructive | push / merge / tag / rebase / reset / stash | (任何 wave 都不跑) |
| 任何 network 调用 | 除 case 39 内部 backend probe | (按需) |

## 5. 推荐使用场景

| 场景 | 推荐命令 |
|------|---------|
| 任何 source 改动后, commit 前快速校验 | `./scripts/run_all_smoke.sh` |
| 计划运行 wave 实施前, 先看一遍要跑啥 | `./scripts/run_all_smoke.sh --dry-run` |
| CI 集成 (Wave 6+ 落地后) | `./scripts/run_all_smoke.sh && python3 scripts/smoke_check.py` (后者跑全 158 case) |
| case 39 fingerprint 漂移调查 | 退出码 2 时, 看 `case 39 fingerprint = ...` 行的实际值, 与 `870f99ed...` 对比 |

## 6. 与 `red-lines.md` 联动

退出码 **2** = case 39 fingerprint 漂移 = 永久红线触发 (`docs/reference/red-lines.md §3`):

| 步骤 | 动作 |
|------|------|
| 1 | **立即停下**, 不擅自回滚 / 不擅自修复 |
| 2 | 报告: 当前 fingerprint 实际值 + 期望值 (`870f99ed494d3d145ed2eb1368132299`) |
| 3 | 报告: 当前 worktree git status / 最近 commit 链 |
| 4 | 等 Allen 拍板 (回滚 / 重新计算 baseline / 调查根因) |

## 7. 维护责任

- **本文件**: `docs/reference/smoke-runner.md`
- **脚本**: `scripts/run_all_smoke.sh` (Wave 4 阶段 1 落地)
- **更新时机**:
  - 新增 wave smoke 文件 → glob 模式自动覆盖, 无需改本文件
  - 新增非 wave/wave0_perm/i18n/audit 类型 smoke → 必须在 `run_all_smoke.sh` 显式追加 + 更新本文件 §3
  - case 39 fingerprint 期望值变化 (Allen 重新认定 baseline) → 必须更新 `EXPECTED_FINGERPRINT` 常量 + 本文件 §2 + `red-lines.md §2`
  - R8.2 whitelist (`scripts/feature-flag-token-whitelist.txt`) 变化 → 同 commit 改 + 同步 `docs/design/bun-feature-flag-system.md §2.2`
  - `KNOWN_DEBT_RESOLVE_ORPHANS` 变化 → 必须 Allen 拍板 + 同步本文件 §9
  - Layer boundary 规则变化 → 同 commit 改 `scripts/layer-boundary-rules.json` + `docs/reference/layer-boundary-rules.md`
  - **stream-json schema/whitelist 变化** (CWB-3) → 同 commit 改 `entrypoints/sdk/{core,control}Schemas.ts` + `scripts/stream-json-schema-whitelist.txt` + `docs/reference/protocol-contract.md`; 漂移即 step 18 fail 阻断 commit

## 8. 已知限制 (Wave 4 阶段 2 v1.1)

- 串行跑 (Wave 6+ 引入并行 runner)
- 无覆盖率报告 (Wave 5 + smoke-coverage-matrix.md 落地后改善)
- 无 CI 集成 (Wave 6+ GitHub Actions 等)
- case 39 fingerprint 提取依赖 smoke_check.py 错误输出格式 (若该格式变, 需同步)
- R8.2 smoke 维护 known debt 清单 (Wave 5 Phase 2 后清空, 当前 `KNOWN_DEBT_RESOLVE_ORPHANS = frozenset()`), 新发现 orphan 必须 Allen 拍板加入或清除
- layer boundary audit 第一版只阻断高风险未来目录, 不把现有 CLI Core UI 的全部 `components/**` 视作外部 Surface

## 9. Known debt 策略

R8.2 smoke 区分两类"不一致":

| 类别 | 行为 | 例子 |
|------|------|------|
| **新增/缺失 feature() token** | smoke fail (退出 1) — 必须同步 whitelist + design doc | 新加 `feature('X')` 但忘加 whitelist |
| **resolve() 有 callsite 但无对应 feature() token (orphan)** | 默认 fail (dead code); 仅在 `KNOWN_DEBT_RESOLVE_ORPHANS` 内的允许 (smoke pass + 输出 known debt 行) | 历史例: `BRIDGE_MODE` (Bridge 子系统 Wave 1.5 删除尾巴, Wave 5 Phase 2 已清, 当前清单为空) |

加入 `KNOWN_DEBT_RESOLVE_ORPHANS` 必须 Allen 拍板 — 不允许通过加白名单绕过 dead code 检测。

---

*— smoke runner 使用说明 v1.1 / Wave 4 阶段 2 R8.2 落地. 配套脚本 scripts/run_all_smoke.sh + scripts/wave4_r8_feature_flag_smoke.py + scripts/feature-flag-token-whitelist.txt.*
