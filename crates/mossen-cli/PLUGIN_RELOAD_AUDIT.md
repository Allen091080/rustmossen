# Plugin Reload 联动审计 / Phase 2-4

## reload-plugins 命令实现位置
- 定义：`crates/mossen-commands/src/reload_plugins.rs:9` — `ReloadPluginsDirective`
- Directive 实现：同文件第 115 行 `impl Directive for ReloadPluginsDirective`
- 注册位置：`crates/mossen-commands/src/lib.rs:245` — `Box::new(reload_plugins::ReloadPluginsDirective)`

## reload 后触发的下游事件

### 1. 清 skill 缓存
- **状态：✅ 存在**
- 证据：`crates/mossen-skills/src/dynamic.rs:298` — `s.conditional_skills.clear(); s.activated_conditional_names.clear();`
- 但需要验证 ReloadPlugins 命令实际调用链是否到达这里。

### 2. 重扫 skill 目录
- **状态：🟡 部分实现**
- 证据：`crates/mossen-skills/src/dynamic.rs:326` — `discover_skill_dirs_for_paths` 存在
- 但 ReloadPluginsDirective 实现中未直接调用 skill 重新发现。需要确认命令实现是否触发了 `discover_skill_dirs_for_paths`。

### 3. 通知 TUI 刷新工具列表
- **状态：🔴 未实现**
- 证据：`mossen-agent/src/` 和 `mossen-tui/src/` 中无 `skills_loaded` / `on_dynamic_skills_loaded` 的订阅消费
- `crates/mossen-skills/src/registry.rs:125` 会 emit `skills_loaded` signal，但没有任何渲染层/agent 层在监听该信号

### 4. MCP 重连
- **状态：🔴 未实现**
- 证据：MCP 连接管理模块中无 reload 触发的重连逻辑

## 是否有 EventBus / 信号机制
- **判定：部分存在**
- `crates/mossen-skills/src/registry.rs:68` — `skills_loaded: Signal` 是内部信号
- `crates/mossen-skills/src/registry.rs:212` — `on_skills_loaded` 可注册回调
- `crates/mossen-skills/src/dynamic.rs:257` — `emit_skills_loaded()` 被调用
- `crates/mossen-skills/src/dynamic.rs:276` — `on_dynamic_skills_loaded` 可订阅
- 但**没有消费端**：agent 主循环、TUI 渲染循环都没有订阅这些信号

## 缺失项 fix 提议
1. **ReloadPluginsDirective 实现中补齐 skill 重扫**：调用 `discover_skill_dirs_for_paths` 后再调用 `activate_conditional_skills_for_paths`
2. **agent 主循环订阅 skills_loaded 信号**：收到后刷新工具注册表
3. **TUI 渲染循环订阅 skills_loaded 信号**：收到后重新渲染工具列表
4. **MCP 连接管理增加 reload 触发重连**：收到 skills_loaded 后断开旧 MCP 连接，重新初始化
