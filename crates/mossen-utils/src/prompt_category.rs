//! # prompt_category — 提示分类
//!
//! 对应 TypeScript `utils/promptCategory.ts`。

/// 获取 agent 的查询来源分类。
pub fn get_query_source_for_agent(agent_type: Option<&str>, is_built_in_agent: bool) -> String {
    if is_built_in_agent {
        match agent_type {
            Some(t) => format!("agent:builtin:{}", t),
            None => "agent:default".to_string(),
        }
    } else {
        "agent:custom".to_string()
    }
}

/// 获取 REPL 的查询来源分类。
pub fn get_query_source_for_repl(output_style: Option<&str>, default_style: &str) -> String {
    let style = output_style.unwrap_or(default_style);
    if style == default_style {
        "repl_main_thread".to_string()
    } else {
        // 所有内置样式
        format!("repl_main_thread:outputStyle:{}", style)
    }
}
