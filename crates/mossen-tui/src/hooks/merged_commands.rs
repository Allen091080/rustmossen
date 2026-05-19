//! Merged commands hook (useMergedCommands.ts).
//! Combines built-in commands with plugin-provided commands.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CommandDef {
    pub name: String,
    pub description: String,
    pub source: CommandSource,
    pub hidden: bool,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandSource { Builtin, Plugin(String), User }

#[derive(Debug, Clone)]
pub struct MergedCommandsState {
    pub commands: HashMap<String, CommandDef>,
    pub alias_map: HashMap<String, String>,
}

impl MergedCommandsState {
    pub fn new() -> Self { Self { commands: HashMap::new(), alias_map: HashMap::new() } }
    pub fn register(&mut self, cmd: CommandDef) {
        for alias in &cmd.aliases { self.alias_map.insert(alias.clone(), cmd.name.clone()); }
        self.commands.insert(cmd.name.clone(), cmd);
    }
    pub fn unregister(&mut self, name: &str) {
        if let Some(cmd) = self.commands.remove(name) {
            for alias in &cmd.aliases { self.alias_map.remove(alias); }
        }
    }
    pub fn resolve(&self, name: &str) -> Option<&CommandDef> {
        self.commands.get(name).or_else(|| self.alias_map.get(name).and_then(|n| self.commands.get(n)))
    }
    pub fn visible_commands(&self) -> Vec<&CommandDef> {
        self.commands.values().filter(|c| !c.hidden).collect()
    }
}
impl Default for MergedCommandsState { fn default() -> Self { Self::new() } }
