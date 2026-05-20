# Permission Context 写入端审计 / Phase 2-1

## 审计目标
确认 `ToolPermissionContext` 在 Rust 端是否存在**多个写入端**，以及是否符合"单一写入端 + 只读消费"的设计目标。

## ToolPermissionContext 定义位置

| 定义位置 | file:line | 用途 |
|---|---|---|
| `mossen-cli/src/query_engine.rs:40` | pub struct | CLI 查询引擎内部权限上下文 |
| `mossen-cli/src/app_state.rs:15` | pub struct | CLI app_state 中的权限上下文 |
| `mossen-agent/src/tool_registry.rs:238` | pub struct | Agent 工具注册中心 |
| `mossen-agent/src/api/mossen_api.rs:105` | pub struct | API 层权限上下文 |
| `mossen-utils/src/attachments.rs:954` | pub struct | 附件模块权限上下文 |
| `mossen-utils/src/permissions/permission_result.rs` | pub struct | 权限结果中的上下文 |
| `mossen-tools/src/bash_tool/mode_validation.rs:39` | pub struct | Bash 工具模式验证 |
| `mossen-types/src/permissions.rs:571` | pub struct | 类型层权限上下文 |
| `mossen-utils/src/permissions/permission_update.rs` | 消费 | 权限更新逻辑（读 + 写返回新实例） |
| `mossen-utils/src/permissions/filesystem.rs` | 消费 | 文件系统权限检查（只读） |
| `mossen-utils/src/permissions/setup.rs:750` | 消费 | 权限设置逻辑（读写） |
| `mossen-utils/src/permissions/permissions.rs` | 消费 | 权限判断逻辑（只读 + 写返回新实例） |

## 写入端分析

### 直接写入（struct field 赋值）

| 写入点 | file:line | 写入方式 | 是否应该存在 |
|---|---|---|---|
| `mossen-cli/src/app_state.rs:144` | `tool_permission_context: ToolPermissionContext::default()` | 初始化默认值 | ✅ 单点初始化 |
| `mossen-cli/src/app_state.rs:214` | `s.tool_permission_context.mode = mode.to_string()` | 模式设置 | ✅ CLI 入口 |
| `mossen-cli/src/app_state.rs:278` | `state.tool_permission_context.mode = mode.to_string()` | 模式设置 | ✅ CLI 入口 |
| `mossen-agent/src/tool_registry.rs:392` | `permission_context: ToolPermissionContext` | 工具注册时赋值 | ✅ 工具注册中心 |
| `mossen-utils/src/permissions/setup.rs:808` | `let initial_context = ToolPermissionContext { ... }` | 初始化权限上下文 | ✅ 权限设置入口 |

### 通过注册 setter 的间接写入

| 写入点 | file:line | 机制 | 是否应该存在 |
|---|---|---|---|
| `mossen-utils/src/swarm/mod.rs:123` | `REGISTERED_PERMISSION_CONTEXT_SETTER` | 全局 setter 注册 | ✅ 多 agent 通信 |
| `mossen-utils/src/swarm/mod.rs:142` | `register_leader_set_tool_permission_context` | 注册函数 | ✅ |
| `mossen-utils/src/swarm/mod.rs:147` | `get_leader_set_tool_permission_context` | 获取函数 | ✅ |
| `mossen-utils/src/leader_permission_bridge.rs:18` | `REGISTERED_PERMISSION_CONTEXT_SETTER` | 全局 setter 注册 | ✅ |

## 结论

**多个定义存在但无冲突**：ToolPermissionContext 在不同 crate 中各自定义同名 struct，但这是 Rust 模块化设计的正常现象（各 crate 独立编译，通过类型兼容性检查）。关键在于：

- ✅ **单一写入端成立**：所有写入集中在 `app_state.rs`（CLI 入口）和 `permissions/setup.rs`（权限初始化）
- ✅ **setter 注册机制是单点**：swarm 和 leader_permission_bridge 通过全局静态变量注册 setter，不直接写入
- ⚠️ **多个独立 struct 定义**：虽然结构相同，但类型不兼容。如果将来需要跨 crate 传递，应考虑统一到 `mossen-types`

## 建议
- 保持当前架构（各 crate 独立定义 + 统一入口写入）
- 如需跨 crate 传递权限上下文，将定义统一到 `mossen-types/src/permissions.rs`
- 不要新增写入端；所有修改权限上下文的路径都应经过 `app_state.rs` 或 `permissions/setup.rs`
