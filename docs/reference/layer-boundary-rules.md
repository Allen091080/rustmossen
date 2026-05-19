# Layer Boundary Audit Rules

> 脚本: `scripts/layer_boundary_audit.py`
> 规则: `scripts/layer-boundary-rules.json`
> 目的: 防止 Workbench、Web、Mobile、Extensions 或 Protocol 层直接 import Core 私有实现。

## 1. 为什么需要这个审计

Wave 4 后 Mossen 的下一阶段会进入 Workbench、扩展系统和自进化。最大风险不是某个 UI 写丑，而是新 Surface 或扩展为了省事直接 import Core 内部文件，导致 CLI 二进制、Core runtime 和 Workbench 代码混成一团。

这个审计先做最保守的高风险阻断:

- Workbench/Surface 不得 import Core internals。
- 外部 Extensions 不得 import permission/tool internals。
- Protocol/SDK 不得 import React/Ink UI。
- Core 不得 import Workbench/Surface implementation。

## 2. 规则文件结构

`scripts/layer-boundary-rules.json` 使用以下字段:

| 字段 | 含义 |
| --- | --- |
| `sourceExtensions` | 扫描的源码扩展名。 |
| `ignoreDirs` | 跳过目录。 |
| `rules[].from` | import 发起文件 glob。 |
| `rules[].to` | import 目标文件 glob。 |
| `rules[].reason` | fail 时输出的原因。 |
| `allowlist` | 明确允许的历史例外，第一版为空。 |

glob 均使用仓库相对路径，例如 `components/Workbench/**`、`query.ts`、`tools/**`。

## 3. 当前第一版的取舍

第一版不会把现有 `components/**` 全部视为 Workbench/Surface，因为当前 CLI 的 React/Ink 组件仍有大量 Core UI 职责。如果直接把全部 `components/**` 列入 Surface，会产生大量历史误报。

因此第一版重点保护未来新增目录:

- `components/Workbench/**`
- `workbench/**`
- `surfaces/**`
- `apps/workbench/**`
- `web/**`
- `mobile/**`
- `extensions/**`
- `skills/external/**`
- `plugins/external/**`

后续若新增真实 Workbench repo 或扩展目录，必须同步扩展这些 glob。

## 4. 本地运行

```bash
python3 scripts/layer_boundary_audit.py
```

预期输出:

```text
PASS: layer boundary audit
  scanned files : <n>
  imports seen  : <n>
  rules         : <n>
```

任何违规会输出:

```text
FAIL: layer boundary audit found violations
  <file>:<line> imports <target>
    rule: <id>
    reason: <reason>
```

## 5. 接入统一验证

`scripts/run_all_smoke.sh` 已包含:

```bash
python3 scripts/layer_boundary_audit.py
```

后续 Workbench / Extension / Protocol 相关任务，commit 前至少跑:

```bash
python3 scripts/layer_boundary_audit.py
bash scripts/run_all_smoke.sh --dry-run
bash scripts/run_all_smoke.sh
```

## 6. 维护规则

| 场景 | 必须做 |
| --- | --- |
| 新增 Workbench/Web/Mobile 目录 | 同 commit 更新 `rules[].from`。 |
| 新增外部 extension 目录 | 同 commit 更新 extension from globs。 |
| 新增稳定 protocol 目录 | 检查是否要加入 protocol no-ui 规则。 |
| 确认历史例外 | 不要直接加 allowlist，先写 NEEDS-DESIGN 并等 Allen 拍板。 |
| 审计误报 | 优先修规则粒度，不用全局关闭规则。 |

## 7. 红线

- 不允许为了让 audit pass 去删除或隐藏真实违规。
- 不允许通过宽泛 allowlist 绕过 Core 边界。
- 不允许 Workbench 通过相对路径、`src/` alias 或 dynamic import 绕过审计。
- 不允许修改 `scripts/smoke_check.py` 来规避边界问题。

