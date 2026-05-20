# Skill Dynamic Discovery 审计 / Phase 2-2

## 审计结果

### 现有实现
- `mossen-skills/src/discovery.rs:19` — `discover_skill_dirs_for_paths` **已实现**
- `mossen-skills/src/dynamic.rs:326` — `discover_skill_dirs_for_paths` **已实现**
- `mossen-skills/src/dynamic.rs:442` — `activate_conditional_skills_for_paths` **已实现**
- `mossen-skills/src/dynamic.rs:257` — `emit_skills_loaded()` 信号机制存在
- `mossen-skills/src/dynamic.rs:276` — `on_dynamic_skills_loaded` 订阅接口存在

### 调用覆盖
| 调用点 | 调用了吗 | 位置 |
|---|---|---|
| dialogue.rs 工具结果处理后 | ❌ 未调用 | mossen-agent 不依赖 mossen-skills |
| CLI REPL 启动路径 | ❌ 未调用 | 启动时未触发 skill 发现 |
| Tool dispatcher 中 | ❌ 未调用 | 工具调度层无 skill 发现调用 |

### 缺口
- `discover_skill_dirs_for_paths` 和 `activate_conditional_skills_for_paths` 存在但**从未被调用**
- 没有路径触发 skill 目录的运行时发现
- `skills_loaded` 信号发出后无消费端

### 接线建议
1. 在 mossen-cli 的 REPL 或 main.rs 启动路径中调用 `discover_skill_dirs_for_paths`
2. 在 tool 执行完成后（CLI 层回调）调用 `activate_conditional_skills_for_paths`
3. 或在 mossen-agent 中添加 mossen-skills 依赖后在 dialogue.rs 中接线
