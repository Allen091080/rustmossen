//! `/usage` — View local session usage.
//!
//! Shows token, cost, duration, and file-change counters recorded by the
//! current local session.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Usage dashboard command — local session consumption info.
pub struct UsageDirective;

#[async_trait]
impl Directive for UsageDirective {
    fn name(&self) -> &str {
        "usage"
    }

    fn description(&self) -> &str {
        "View local session token and cost usage"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /usage [options]\n\n\
                 View local token, cost, duration, and file-change counters recorded by this session.\n\n\
                 Options:\n\
                   --breakdown  Show per-model token breakdown\n\
                   --total      Show only totals"
                    .to_string(),
            ));
        }

        let snapshot = &ctx.cost_snapshot;
        let mut output = String::from("Usage Summary\n");
        output.push_str("=============\n\n");

        if !snapshot.has_recorded_usage() {
            output.push_str("No session usage has been recorded yet.\n");
            output.push_str("Run a model turn first, then retry /usage.\n");
            return Ok(CommandResult::Text(output));
        }

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
        let total_tokens = total_input
            .saturating_add(total_output)
            .saturating_add(total_cache_read)
            .saturating_add(total_cache_creation);

        if snapshot.model_usage.is_empty() {
            output.push_str("Token usage:      not available on this UI path\n");
        } else {
            output.push_str(&format!("Total tokens:     {}\n", total_tokens));
            output.push_str(&format!("Input tokens:     {}\n", total_input));
            output.push_str(&format!("Output tokens:    {}\n", total_output));
            output.push_str(&format!("Cache read:       {}\n", total_cache_read));
            output.push_str(&format!("Cache creation:   {}\n", total_cache_creation));
        }
        output.push_str(&format!(
            "Estimated cost:   {}\n",
            format_cost(snapshot.total_cost_usd)
        ));
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

        let breakdown = args
            .iter()
            .any(|arg| matches!(*arg, "--breakdown" | "breakdown" | "detailed"));
        let total_only = args.iter().any(|arg| matches!(*arg, "--total" | "total"));
        if breakdown && !total_only && !snapshot.model_usage.is_empty() {
            output.push_str("\nBy model\n");
            output.push_str("--------\n");
            let mut models: Vec<_> = snapshot.model_usage.iter().collect();
            models.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (model, usage) in models {
                output.push_str(&format!(
                    "{}: {} in / {} out / {} cache read / {} cache create / {}\n",
                    model,
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.cache_read_input_tokens,
                    usage.cache_creation_input_tokens,
                    format_cost(usage.cost_usd)
                ));
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
    use crate::context::{CommandContext, CommandCostModelUsage, CommandCostSnapshot};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context(cost_snapshot: CommandCostSnapshot) -> CommandContext {
        CommandContext {
            cwd: PathBuf::from("."),
            is_non_interactive: true,
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
    fn usage_reports_unrecorded_state_instead_of_fake_plan_data() {
        let output = text(
            tokio_test::block_on(UsageDirective.execute(&[], &test_context(Default::default())))
                .expect("usage command"),
        );

        assert!(
            output.contains("No session usage has been recorded"),
            "{output}"
        );
        assert!(!output.contains("Plan: Standard"), "{output}");
        assert!(!output.contains("Days remaining"), "{output}");
    }

    #[test]
    fn usage_reports_injected_runtime_snapshot() {
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
                cost_usd: 0.42,
                ..Default::default()
            },
        );

        let output = text(
            tokio_test::block_on(UsageDirective.execute(&["--breakdown"], &test_context(snapshot)))
                .expect("usage command"),
        );

        assert!(output.contains("Total tokens:     1650"), "{output}");
        assert!(output.contains("Estimated cost:   $0.42"), "{output}");
        assert!(output.contains("example-fast-highspeed"), "{output}");
    }

    #[test]
    fn usage_does_not_fake_zero_tokens_when_only_cost_is_available() {
        let snapshot = CommandCostSnapshot {
            total_cost_usd: 0.03,
            ..Default::default()
        };

        let output = text(
            tokio_test::block_on(UsageDirective.execute(&[], &test_context(snapshot)))
                .expect("usage command"),
        );

        assert!(
            output.contains("Token usage:      not available"),
            "{output}"
        );
        assert!(!output.contains("Total tokens:     0"), "{output}");
        assert!(output.contains("Estimated cost:   $0.03"), "{output}");
    }
}
