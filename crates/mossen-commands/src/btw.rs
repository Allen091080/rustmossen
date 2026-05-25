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

    // In the full implementation, this would:
    // 1. Save btwUseCount to global config
    // 2. Build CacheSafeParams for the fork
    // 3. Run the side question through the model
    // 4. Stream the response with scroll support
    //
    // For now, acknowledge the question and explain this is a side channel
    let system_prompt = build_side_question_system_prompt(ctx);
    Ok(format!(
        "Side question received: \"{}\"\n\n\
         [This would invoke the model with a forked context to answer without \
         disrupting the main conversation thread.]\n\n\
         System prompt: {}",
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
        Ok(CommandResult::Text(result))
    }
}
