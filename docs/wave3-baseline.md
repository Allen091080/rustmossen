# Wave 3 第一批 — 开工前基线记录

**记录时间**: 2026-04-29 09:30+
**记录人**: 主 agent (执行 Allen 批准的 Wave 3 第一批)

## 起点

| 项 | 值 |
|---|---|
| Main HEAD | `c8a08320f8ea470b91dbabebd5b67a53627d4faa` (`docs: Wave 2 9 slices 完成收口记录`) |
| 起点 tag | `wave2-user-type-cleanup-20260429` |
| 主仓 git status | clean (零改动, 仅 untracked `harness全链路测试.md.bak-20260425`) |
| Wave 3 worktree | `/Users/allen/Documents/aiproject/mossensrc-wave3-cleanup` |
| Wave 3 branch | `worktree/wave3-cleanup` |
| Wave 3 worktree HEAD | `c8a0832` (= main, 起点) |

## 基线 fingerprint

- **case 39 fingerprint** (排除 status_stdout/status_stderr/text_output 三运行时态字段后 md5):
  - `870f99ed494d3d145ed2eb1368132299` ✓ (与 Wave 2 完成时一致)
  - 计算方法: `python3 scripts/smoke_check.py --only custom_backend_auth_runtime_audit` → 提取 JSON → strip 3 字段 → `json.dumps(sort_keys=True)` → md5

## 已并存 worktree (供并发评估)

```
/Users/allen/Documents/aiproject/mossensrc                       c8a0832 [main]
/Users/allen/Documents/aiproject/mossensrc-growthbook-migration  f71e55d [worktree/stage1-cli-baseline]
/Users/allen/Documents/aiproject/mossensrc-ux-w2-command         ab71e89 [worktree/ux-w2-command-description]
/Users/allen/Documents/aiproject/mossensrc-ux-wave1              871353a [ux-wave1-i18n-base]
/Users/allen/Documents/aiproject/mossensrc-wave2-user-type       c8a0832 [worktree/wave2-user-type]
/Users/allen/Documents/aiproject/mossensrc-wave3-cleanup         c8a0832 [worktree/wave3-cleanup]  ← 本次新建
```

## Allen 批准范围

仅 3 slice (低风险):
- Slice 5 — 注释/文档脱敏
- Slice 1 — UI tsx 死分支 6 处
- Slice 6 — P0 [ANT-ONLY] 5 处用户感知

## Allen 已答决策点

- D8 = A (P0 [ANT-ONLY] 本周修)
- D9 = A (Slice 5 作预热)
- 其余 D1-D7 / D10-D12 暂不拍板, 不得启动相关 slice

## Allen 红线 (绝对禁止)

- ❌ 不触碰 i18n / smoke_check.py / memory / runtime API / case 39 / UX worktree
- ❌ 不 push / merge / tag / rebase / reset / force push
- ❌ 不进入 Slice 3 / Slice 2 / Slice 7 / Slice 8 / USER_TYPE 收敛

## 验收要求

- 每 slice 后: typecheck:diff + lint:diff + 对应 smoke
- 第一批完成后: 总验收 (case 39 fingerprint 比对 + 9 wave2 smoke + LLM 冒烟)

## 停下条件

任一触发立即停下汇报, 不得自行扩大范围:
- 验证失败
- 文件范围超出施工包
- 发现与施工包不一致
