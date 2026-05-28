//! `/review` — Review a pull request (prompt command).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Review directive — triggers a code review via model prompt.
pub struct ReviewDirective;

/// Generate the local review prompt with the given PR number/args.
fn local_review_prompt(args: &str) -> String {
    format!(
        r#"You are an expert code reviewer. Follow these steps:

1. If no PR number is provided in the args, run `gh pr list` to show open PRs
2. If a PR number is provided, run `gh pr view <number>` to get PR details
3. Run `gh pr diff <number>` to get the diff
4. Analyze the changes and provide a thorough code review that includes:
   - Overview of what the PR does
   - Analysis of code quality and style
   - Specific suggestions for improvements
   - Any potential issues or risks

Keep your review concise but thorough. Focus on:
- Code correctness
- Following project conventions
- Performance implications
- Test coverage
- Security considerations

Format your review with clear sections and bullet points.

PR number: {}"#,
        args
    )
}

#[async_trait]
impl Directive for ReviewDirective {
    fn name(&self) -> &str {
        "review"
    }

    fn description(&self) -> &str {
        "Review a pull request"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Prompt
    }

    fn argument_hint(&self) -> &str {
        "[PR number]"
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        let args_str = args.join(" ").trim().to_string();
        let prompt = local_review_prompt(&args_str);
        Ok(CommandResult::Text(prompt))
    }
}
