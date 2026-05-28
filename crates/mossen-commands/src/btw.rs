//! `/btw` — Ask a quick side question without interrupting the main conversation.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Btw directive — ask a side question without interrupting the main conversation.
pub struct BtwDirective;

/// Build the system prompt for the side question fork.
fn build_side_question_system_prompt(ctx: &CommandContext) -> String {
    format!(
        "You are {}, a helpful coding assistant. The user is asking a quick side question. \
         Answer concisely and helpfully without disrupting their main workflow. \
         Keep responses brief and focused.",
        ctx.product_name
    )
}

/// Strip in-progress assistant messages from the context.
fn strip_in_progress_messages(messages: &[String]) -> Vec<String> {
    if let Some(last) = messages.last() {
        if last.starts_with("[assistant:in-progress]") {
            return messages[..messages.len() - 1].to_vec();
        }
    }
    messages.to_vec()
}

/// Execute the side question flow.
async fn execute_btw_flow(args: &[&str], ctx: &CommandContext) -> Result<String> {
    let question = args.join(" ");
    let question = question.trim();

    if question.is_empty() {
        return Ok("Usage: /btw <your question>".to_string());
    }

    let system_prompt = build_side_question_system_prompt(ctx);
    Ok(format!(
        "Cannot run side question \"{}\" from this command runner. No side-channel model runner or forked context store is attached.\nSystem prompt preview: {}",
        question,
        &system_prompt[..system_prompt.len().min(100)]
    ))
}

#[async_trait]
impl Directive for BtwDirective {
    fn name(&self) -> &str {
        "btw"
    }

    fn description(&self) -> &str {
        "Ask a quick side question without interrupting the main conversation"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn argument_hint(&self) -> &str {
        "<question>"
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let result = execute_btw_flow(args, ctx).await?;
        if args.is_empty() {
            Ok(CommandResult::Text(result))
        } else {
            Ok(CommandResult::Error(result))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context() -> CommandContext {
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
            cost_snapshot: Default::default(),
        }
    }

    #[test]
    fn btw_directive_does_not_claim_side_channel_without_runner() {
        let output =
            tokio_test::block_on(BtwDirective.execute(&["what", "changed"], &test_context()))
                .expect("btw command");

        let CommandResult::Error(text) = output else {
            panic!("btw should fail closed without side-channel runner");
        };
        assert!(text.contains("Cannot run side question"), "{text}");
        assert!(!text.contains("This would invoke"), "{text}");
    }
}
