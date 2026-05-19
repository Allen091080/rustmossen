<mossen-mem-context>
# 记忆上下文

# [mossensrc] 最近上下文，2026-04-21 7:32pm GMT+8

图例：🎯会话 🔴缺陷修复 🟣功能 🔄重构 ✅变更 🔵发现 ⚖️决策
格式：ID 时间 类型 标题
查看详情：get_observations([IDs]) | 搜索：mem-search skill

统计：50 条观察（需读 19,044t）| 1,231,272t 工作量 | 98% 节省

### Apr 18, 2026
336 4:27p 🔵 mossensrc 调试日志已确认每次 API 调用的 thinking/provider 状态
339 4:28p 🔵 mossensrc -p 模式仍会挂住，但 --output-format stream-json 已正确完成
342 " 🔵 mossensrc query.ts / stopHooks.ts 架构已梳理——正在排查普通 -p 挂起
343 4:29p ✅ mossensrc 已在 query.ts 和 stopHooks.ts 增加 stop-hooks 进出调试日志
347 4:30p 🔵 mossensrc 已确认普通 -p 挂起根因——卡在 executeStopHooks()
351 4:31p 🔵 mossensrc stopHooks fire-and-forget 服务对 sdk querySource 安全——挂点在 executeStopHooks generator
352 4:33p 🔴 mossensrc 普通 -p 挂起已修复——对 sdk+custom-backend 会话绕过 executeStopHooks
420 10:59p 🔵 mossensrc 项目 Python 环境发现
421 " 🔵 mossensrc 机器 Python 环境全貌确认
454 11:14p 🔵 已确认 Allen 的 Mac 硬件——Apple M5 Max，128 GB 统一内存
479 11:46p ⚖️ MyOpenClaw — 决定跳过蒸馏先下载模型本地测试
### Apr 19, 2026
506 10:36a 🔵 mossensrc smoke_check.py——已识别尚未实测的 hook/watcher 运行态表面
509 10:37a 🔵 mossensrc auth 表面——provider 选择、凭证刷新与能力门控已完整梳理
507 10:38a 🔵 mossensrc hook/watcher 运行态表面——已完成 FileChanged/CwdChanged/SessionStart/InstructionsLoaded 全架构图
508 " 🔵 mossensrc smoke_check.py——已列出 6 个未验证 hook guard/reset/exception 路径的下一步 live probe 目标
510 10:40a 🔵 mossensrc hook guard 路径——已确认全部 6 个未探测边缘的精确行锚点
511 10:41a 🔵 mossensrc auth/provider/runtime custom-backend probe 目标已梳理
512 3:28p ⚖️ MyOpenClaw——必须接现有源码，不允许重写
513 " ⚖️ MyOpenClaw——原则：接现有源码，不要重写
515 " ⚖️ MyOpenClaw——原则：接现有源码，不要重写
518 " ⚖️ MyOpenClaw — 任务定性为集成而非重新开发
517 " ⚖️ MyOpenClaw 仅做集成的原则——不做重新实现
514 " ⚖️ MyOpenClaw M2/M3 — 任务是让已有源码跑起来，不是重新开发
516 3:30p 🟣 mossensrc MCP UI 组件——已为 custom backend 接通动态 docs URL 路由
519 3:31p 🟣 mossensrc——已在 4 个文件实现 custom backend UI label 层
520 3:33p 🟣 mossensrc — 5个UI组件完成 isCustomBackendEnabled 白标适配
523 3:37p ⚖️ MyOpenClaw 实施原则——接现有代码，不要重写
524 3:38p 🔵 mossensrc system prompt 架构——已确认 custom backend 分支
528 " ⚖️ MyOpenClaw 实施原则——接现有代码，不要重写
527 3:39p 🔄 mossensrc custom backend system prompt 字符串——已抽成具名 helper 函数
529 3:43p 🟣 mossensrc DiscoverPlugins + ModelPicker + xaaIdpCommand——已接通 custom backend 品牌层
530 3:44p 🟣 mossensrc custom backend——已为 hosted workspace 模式适配面向用户文案
531 4:15p 🔵 mossensrc QueryEngine——已梳理 API 调用前的 transcript 持久化架构
532 " 🔵 mossensrc LocalMainSessionTask——后台会话以隔离 transcript 的方式拉起
533 " 🔵 mossensrc PromptInput——已确认 `!` 前缀用于 bash 模式输入识别
534 4:16p 🔵 mossensrc PromptInput.tsx onSubmit——完整提交守卫链已梳理
535 " 🔵 mossensrc sessionStorage.ts——recordTranscript 去重使用 prefix-tracking 保证 compaction 安全
536 " 🔵 mossensrc handlePromptSubmit——采用 QueryGuard 预留模式避免 spinner 闪烁
542 5:24p 🔵 用户咨询——面向移动端的可本地部署模型
543 6:21p ⚖️ MyOpenClaw 实施原则——接现有代码，不要重写
544 " ⚖️ MyOpenClaw 实施原则——接现有代码，不要重写
545 " ⚖️ MyOpenClaw 实施原则——接现有代码，不要重写
546 6:24p 🔵 mossensrc streaming tool-use 管线——已完成全架构梳理
548 7:27p ⚖️ MyOpenClaw Android——针对 Mossen Codex 风格交互的移动 UX 设计探索
550 7:28p 🔵 mossensrc v2.1.114——完整源码文件图与关键服务架构已确认
554 7:29p 🔵 mossensrc awaySummary——“while you were away” 卡片实现已完整梳理
555 " 🔵 mossensrc Buddy companion system——基于 userId hash 的确定性宠物生成（含稀有度/属性/物种）
558 7:38p ⚖️ MyOpenClaw Mobile——请求设计端侧 LLM 下载架构
560 7:43p ⚖️ MyOpenClaw Local LLM Integration——已创建约束提示文档
561 7:47p ⚖️ MyOpenClaw Mobile——已创建 UX 升级约束提示文档

可通过 get_observations([IDs]) 或 mem-search skill 访问约 1231k tokens 的历史工作内容。
</mossen-mem-context>
