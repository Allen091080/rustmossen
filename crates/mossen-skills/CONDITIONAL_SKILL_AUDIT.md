# Conditional Skill Activation 审计 / Phase 2-3

## 审计结果

### 现有实现
- `mossen-skills/src/dynamic.rs:225` — `conditional_skills: HashMap<String, CraftCommand>` 数据结构存在
- `mossen-skills/src/dynamic.rs:227` — `activated_conditional_names: HashSet<String>` 去重集存在
- `mossen-skills/src/dynamic.rs:442` — `activate_conditional_skills_for_paths` **已实现**（根据 paths frontmatter 匹配激活）
- `mossen-skills/src/dynamic.rs:542` — `s.conditional_skills.insert` 添加条件技能

### 调用覆盖
| 调用点 | 调用了吗 | 证据 |
|---|---|---|
| tool dispatcher / cwd 变化时 | ❌ 未调用 | 无任何调用点命中 |
| 启动时 | ❌ 未调用 | main.rs/repl.rs 无调用 |
| 文件修改时 | ❌ 未调用 | file_watcher 未接 |

### 缺口
- `activate_conditional_skills_for_paths` 存在但从未被调用
- 条件技能根据路径匹配的激活机制未接通
- 没有 cwd 变化或文件路径变化触发条件技能重新评估

### 接线建议
1. 在 CLI 启动路径中调用一次（初始激活）
2. 在文件 watcher 检测到文件变化时调用（动态激活）
3. 或在 dialogue.rs 工具执行完成后调用（每次工具执行后评估）
