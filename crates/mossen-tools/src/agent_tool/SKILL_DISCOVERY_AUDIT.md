# Skill Dynamic Discovery 审计 / Phase 2-2

## 现有实现

- `crates/mossen-skills/src/discovery.rs:19` — 独立 discovery helper 存在。
- `crates/mossen-skills/src/dynamic.rs:330` — runtime `discover_skill_dirs_for_paths` 已实现。
- `crates/mossen-skills/src/dynamic.rs:382` — `add_skill_directories` 可把发现的 skill 目录加载进动态 skill 集。
- `crates/mossen-skills/src/dynamic.rs:257` — `emit_skills_loaded()` 信号机制存在。
- `crates/mossen-skills/src/dynamic.rs:276` — `on_dynamic_skills_loaded` 订阅接口存在。

## 调用覆盖

| 调用点 | 状态 | 证据 |
|---|---|---|
| non-interactive CLI 启动 | 已调用并加载 | `crates/mossen-cli/src/main.rs:230` 调 `discover_skill_dirs_for_paths`，`main.rs:236` 调 `add_skill_directories` |
| interactive REPL 启动 | 已调用并加载 | `crates/mossen-cli/src/repl.rs:109` 调 `discover_skill_dirs_for_paths`，`repl.rs:116` 调 `add_skill_directories` |
| dialogue.rs 工具结果处理后 | 未调用 | `mossen-agent` 不能依赖 `mossen-skills`，否则与 `mossen-skills -> mossen-agent` 形成循环依赖 |
| tool dispatcher 中 | 未调用 | 工具调度层在 `mossen-agent`，同样受 crate 依赖方向限制 |

## 判定

当前已接通启动时动态发现和加载，覆盖 oneshot/exec 与交互式 REPL 两条入口。运行中“工具产生新路径后立即发现 skill”尚未接到 agent 工具流水线，因为直接在 `dialogue.rs` 调用会造成 crate 循环依赖。

## 后续建议

若需要完全复刻 TS 的每次工具结果触发，应把路径事件作为 SDK/TUI 事件从 `mossen-agent` 发到 `mossen-cli` 或引入一个不依赖 agent 的轻量 skill-event crate，由 CLI 层监听并调用 `mossen-skills`。
