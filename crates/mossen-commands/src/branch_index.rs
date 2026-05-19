//! `/branch` command index — registration metadata and lazy-load entry point.
//!
//! This module defines the command registration metadata for the `/branch` command.
//! The actual implementation logic lives in the corresponding main module.
//! This separation allows for lazy-loading of command implementations to
//! reduce startup time — only metadata is loaded eagerly.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Registration metadata for the `/branch` command.
///
/// This struct provides the command name, description, type classification,
/// and argument hints used by the help system and command router.
/// It does not contain execution logic — that is delegated to the
/// main command module when the command is actually invoked.
pub struct BranchIndexDirective;

#[async_trait]
impl Directive for BranchIndexDirective {
    fn name(&self) -> &str {
        "branch"
    }

    fn description(&self) -> &str {
        "Create or switch to a git branch for the current task"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[branch-name]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        // This index module delegates execution to the main command module.
        // In the current architecture, the command router resolves to the
        // primary directive registered in all_directives() rather than
        // this index entry. This execute implementation exists only to
        // satisfy the Directive trait contract.
        Ok(CommandResult::Empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        let d = BranchIndexDirective;
        assert_eq!(d.name(), "branch");
    }

    #[test]
    fn test_description_not_empty() {
        let d = BranchIndexDirective;
        assert!(!d.description().is_empty());
    }

    #[test]
    fn test_is_immediate() {
        let d = BranchIndexDirective;
        assert!(d.is_immediate());
    }
}
