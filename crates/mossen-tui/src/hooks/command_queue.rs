//! Command queue hook (useCommandQueue.ts).
//!
//! Subscribes to the unified command queue store and returns
//! a frozen snapshot that changes only on mutation.

/// Priority level for queued commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandPriority {
    Later,
    Next,
    Now,
}

/// A queued command waiting to be executed.
#[derive(Debug, Clone)]
pub struct QueuedCommand {
    pub id: String,
    pub text: String,
    pub priority: CommandPriority,
    pub source: CommandSource,
    pub timestamp: u64,
}

/// Source of a queued command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandSource {
    User,
    Task,
    Plugin,
    System,
}

/// State for the command queue.
#[derive(Debug, Clone)]
pub struct CommandQueueState {
    pub commands: Vec<QueuedCommand>,
    pub version: u64,
}

impl CommandQueueState {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            version: 0,
        }
    }

    /// Enqueue a command with the given priority.
    pub fn enqueue(&mut self, command: QueuedCommand) {
        // Insert maintaining priority order
        let pos = self.commands.partition_point(|c| c.priority >= command.priority);
        self.commands.insert(pos, command);
        self.version += 1;
    }

    /// Dequeue the highest priority command.
    pub fn dequeue(&mut self) -> Option<QueuedCommand> {
        if self.commands.is_empty() {
            return None;
        }
        self.version += 1;
        Some(self.commands.remove(0))
    }

    /// Peek at the next command without removing it.
    pub fn peek(&self) -> Option<&QueuedCommand> {
        self.commands.first()
    }

    /// Get current queue length.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Clear all commands.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.version += 1;
    }

    /// Get a snapshot of the current queue.
    pub fn snapshot(&self) -> &[QueuedCommand] {
        &self.commands
    }
}

impl Default for CommandQueueState {
    fn default() -> Self {
        Self::new()
    }
}
