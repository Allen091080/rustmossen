//! `/cost` — Show token usage and estimated cost for this session.
//!
//! Displays a breakdown of token consumption, API calls, and
//! estimated monetary cost for the current session.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{
    CommandContext, CommandCostModelUsage, CommandResult, Directive, DirectiveType,
};

/// Cost/Meter command — session resource usage tracking.
///
/// Shows:
/// - Total tokens consumed (input + output)
/// - Number of API calls made
/// - Estimated cost based on model pricing
/// - Cost per message breakdown
pub struct MeterDirective;

#[async_trait]
impl Directive for MeterDirective {
    fn name(&self) -> &str {
        "cost"
    }

    fn description(&self) -> &str {
        "Show token usage and estimated cost for this session"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /cost [options]\n\n                 Show token usage and cost for this session.\n\n                 Options:\n                   --detailed    Show per-message breakdown\n                   --total       Show only session totals"
                    .to_string(),
            ));
        }

        let detailed = args
            .iter()
            .any(|arg| matches!(*arg, "--detailed" | "detailed"));
        let total_only = args.iter().any(|arg| matches!(*arg, "--total" | "total"));
        let snapshot = &ctx.cost_snapshot;

        let mut output = String::from("Session Cost Summary\n");
        output.push_str("====================\n\n");

        if !snapshot.has_recorded_usage() {
            output.push_str("No token usage has been recorded for this session yet.\n");
            output.push_str("Run a model turn first, then retry /cost.\n");
            return Ok(CommandResult::Text(output));
        }

        output.push_str(&format!(
            "Total cost:       {}\n",
            format_cost(snapshot.total_cost_usd)
        ));
        if snapshot.model_usage.is_empty() {
            output.push_str("Token usage:      not available on this UI path\n");
        } else {
            let total_input: u64 = snapshot
                .model_usage
                .values()
                .map(|usage| usage.input_tokens)
                .sum();
            let total_output: u64 = snapshot
                .model_usage
                .values()
                .map(|usage| usage.output_tokens)
                .sum();
            let total_cache_read: u64 = snapshot
                .model_usage
                .values()
                .map(|usage| usage.cache_read_input_tokens)
                .sum();
            let total_cache_creation: u64 = snapshot
                .model_usage
                .values()
                .map(|usage| usage.cache_creation_input_tokens)
                .sum();
            let total_web_search: u64 = snapshot
                .model_usage
                .values()
                .map(|usage| usage.web_search_requests)
                .sum();
            let total_tokens = total_input
                .saturating_add(total_output)
                .saturating_add(total_cache_read)
                .saturating_add(total_cache_creation);

            output.push_str(&format!("Input tokens:     {}\n", total_input));
            output.push_str(&format!("Output tokens:    {}\n", total_output));
            output.push_str(&format!("Cache read:       {}\n", total_cache_read));
            output.push_str(&format!("Cache creation:   {}\n", total_cache_creation));
            output.push_str(&format!("Total tokens:     {}\n", total_tokens));
            output.push_str(&format!("Web search reqs:  {}\n", total_web_search));
        }
        output.push_str(&format!(
            "API time:         {}\n",
            format_duration(snapshot.total_api_duration_ms)
        ));
        output.push_str(&format!(
            "Tool time:        {}\n",
            format_duration(snapshot.total_tool_duration_ms)
        ));
        output.push_str(&format!(
            "Lines changed:    +{} / -{}\n",
            snapshot.total_lines_added, snapshot.total_lines_removed
        ));
        if snapshot.has_unknown_model_cost {
            output.push_str("Cost status:      partial; at least one model has unknown pricing\n");
        }

        if detailed && !total_only && !snapshot.model_usage.is_empty() {
            output.push_str("\nBy model\n");
            output.push_str("--------\n");
            let mut models: Vec<(&String, &CommandCostModelUsage)> =
                snapshot.model_usage.iter().collect();
            models.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (model, usage) in models {
                output.push_str(&format!(
                    "{}: {} in / {} out / {} cache read / {} cache create / {} searches / {}\n",
                    model,
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.cache_read_input_tokens,
                    usage.cache_creation_input_tokens,
                    usage.web_search_requests,
                    format_cost(usage.cost_usd)
                ));
                if usage.context_window > 0 || usage.max_output_tokens > 0 {
                    output.push_str(&format!(
                        "  context window: {} · max output: {}\n",
                        usage.context_window, usage.max_output_tokens
                    ));
                }
            }
        }

        Ok(CommandResult::Text(output))
    }
}

fn format_cost(cost_usd: f64) -> String {
    if cost_usd < 0.01 {
        format!("${cost_usd:.4}")
    } else {
        format!("${cost_usd:.2}")
    }
}

fn format_duration(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else {
        format!("{:.2}s", ms as f64 / 1_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CommandContext, CommandCostSnapshot};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context(cost_snapshot: CommandCostSnapshot) -> CommandContext {
        CommandContext {
            cwd: PathBuf::from("."),
            is_non_interactive: false,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars: HashMap::new(),
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
            cost_snapshot,
        }
    }

    fn text(result: CommandResult) -> String {
        match result {
            CommandResult::Text(text) => text,
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn cost_reports_unrecorded_state_instead_of_fake_zero_usage() {
        let output = text(
            tokio_test::block_on(MeterDirective.execute(&[], &test_context(Default::default())))
                .expect("cost command"),
        );

        assert!(
            output.contains("No token usage has been recorded"),
            "{output}"
        );
        assert!(!output.contains("API calls:"), "{output}");
    }

    #[test]
    fn cost_reports_injected_runtime_snapshot() {
        let mut snapshot = CommandCostSnapshot {
            total_cost_usd: 0.42,
            total_api_duration_ms: 1_500,
            total_tool_duration_ms: 250,
            total_lines_added: 3,
            total_lines_removed: 1,
            ..Default::default()
        };
        snapshot.model_usage.insert(
            "example-fast-highspeed".to_string(),
            CommandCostModelUsage {
                input_tokens: 1_200,
                output_tokens: 300,
                cache_read_input_tokens: 100,
                cache_creation_input_tokens: 50,
                web_search_requests: 2,
                cost_usd: 0.42,
                context_window: 196_000,
                max_output_tokens: 8_192,
            },
        );

        let output = text(
            tokio_test::block_on(MeterDirective.execute(&["--detailed"], &test_context(snapshot)))
                .expect("cost command"),
        );

        assert!(output.contains("Total cost:       $0.42"), "{output}");
        assert!(output.contains("Input tokens:     1200"), "{output}");
        assert!(output.contains("example-fast-highspeed"), "{output}");
        assert!(output.contains("context window: 196000"), "{output}");
    }
}
