//! `/doctor` command index — registration metadata and lazy-load entry point.
//!
//! This module defines the command registration metadata for the `/doctor` command.
//! The actual implementation logic lives in the corresponding main module.
//! This separation allows for lazy-loading of command implementations to
//! reduce startup time — only metadata is loaded eagerly.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Registration metadata for the `/doctor` command.
///
/// This struct provides the command name, description, type classification,
/// and argument hints used by the help system and command router.
/// It does not contain execution logic — that is delegated to the
/// main command module when the command is actually invoked.
pub struct DoctorIndexDirective;

#[async_trait]
impl Directive for DoctorIndexDirective {
    fn name(&self) -> &str {
        "doctor"
    }

    fn description(&self) -> &str {
        "Run system diagnostics and health checks"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
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
        let d = DoctorIndexDirective;
        assert_eq!(d.name(), "doctor");
    }

    #[test]
    fn test_description_not_empty() {
        let d = DoctorIndexDirective;
        assert!(!d.description().is_empty());
    }

    #[test]
    fn test_is_immediate() {
        let d = DoctorIndexDirective;
        assert!(d.is_immediate());
    }
}
