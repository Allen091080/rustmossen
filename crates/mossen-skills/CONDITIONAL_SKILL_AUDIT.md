# Conditional Skill Activation 审计 / Phase 2-3

## 现有实现

- `crates/mossen-skills/src/dynamic.rs:225` — `conditional_skills: HashMap<String, CraftCommand>` 保存待激活 skill。
- `crates/mossen-skills/src/dynamic.rs:227` — `activated_conditional_names: HashSet<String>` 避免重复激活。
- `crates/mossen-skills/src/dynamic.rs:442` — `activate_conditional_skills_for_paths` 已实现。
- `crates/mossen-skills/src/dynamic.rs:480` — 匹配后从 conditional map 移入 active skill map。
- `crates/mossen-skills/src/dynamic.rs:484` — 激活后 emit `skills_loaded`。

## 调用覆盖

| 调用点 | 状态 | 证据 |
|---|---|---|
| non-interactive CLI 启动 | 已调用 | `crates/mossen-cli/src/main.rs:245` |
| interactive REPL 启动 | 已调用 | `crates/mossen-cli/src/repl.rs:123` |
| cwd 变化时 | 未调用 | slash `/cd` 路径尚未接 `activate_conditional_skills_for_paths` |
| 文件修改/工具结果时 | 未调用 | agent 工具执行层无法直接依赖 `mossen-skills` |

## 判定

当前已接通启动时条件 skill 激活，保证进入会话时 cwd 匹配的 `paths` frontmatter 可以生效。运行时按新文件路径激活还缺一个跨 crate 事件通道。

## 后续建议

把“工具产出路径”和“cwd changed”作为 CLI 层事件处理，比在 agent crate 里直接依赖 `mossen-skills` 更稳；这能保持当前 crate 依赖方向，同时补齐运行时激活。
