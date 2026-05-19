//! Tasks V2 hook (useTasksV2.ts).
//! Manages the v2 task system state.

#[derive(Debug, Clone)]
pub struct TasksV2State {
    pub active: bool,
    pub initialized: bool,
}

impl TasksV2State {
    pub fn new() -> Self { Self { active: false, initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
    pub fn activate(&mut self) { self.active = true; }
    pub fn deactivate(&mut self) { self.active = false; }
    pub fn is_active(&self) -> bool { self.active }
}
impl Default for TasksV2State { fn default() -> Self { Self::new() } }

/// One v2 task slot. Mirrors the TS `Task` shape just enough for the
/// collapse-effect hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskV2 {
    pub id: String,
    pub status: TaskV2Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskV2Status {
    Pending,
    InProgress,
    Completed,
}

/// Which expanded view is currently selected. Translated from the TS
/// `AppState.expandedView` union.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpandedView {
    None,
    Tasks,
    Teammates,
}

/// Outcome of the collapse effect — whether the caller should write a new
/// expanded-view value back into AppState.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapseDecision {
    /// No change required.
    KeepExpanded,
    /// Collapse the expanded-tasks view to `None`.
    CollapseToNone,
}

/// `useTasksV2WithCollapseEffect` — adds a collapse effect on top of
/// `useTasksV2`. When the task list is hidden or holds only completed
/// items the expanded-tasks view is force-collapsed.
///
/// TS source: `useTasksV2WithCollapseEffect()`. Returns the same task list
/// plus the collapse decision so the caller can mutate AppState.
pub fn use_tasks_v2_with_collapse_effect(
    tasks: Option<&[TaskV2]>,
    current_expanded_view: ExpandedView,
) -> CollapseDecision {
    let hidden = tasks.is_none();
    let has_open_tasks = match tasks {
        Some(t) => t.iter().any(|t| t.status != TaskV2Status::Completed),
        None => false,
    };
    if !hidden && has_open_tasks {
        return CollapseDecision::KeepExpanded;
    }
    if current_expanded_view != ExpandedView::Tasks {
        return CollapseDecision::KeepExpanded;
    }
    CollapseDecision::CollapseToNone
}

#[cfg(test)]
mod collapse_tests {
    use super::*;

    #[test]
    fn keeps_expanded_when_has_open_tasks() {
        let tasks = vec![TaskV2 { id: "a".into(), status: TaskV2Status::InProgress }];
        let r = use_tasks_v2_with_collapse_effect(Some(&tasks), ExpandedView::Tasks);
        assert_eq!(r, CollapseDecision::KeepExpanded);
    }

    #[test]
    fn collapses_when_all_completed() {
        let tasks = vec![TaskV2 { id: "a".into(), status: TaskV2Status::Completed }];
        let r = use_tasks_v2_with_collapse_effect(Some(&tasks), ExpandedView::Tasks);
        assert_eq!(r, CollapseDecision::CollapseToNone);
    }

    #[test]
    fn collapses_when_hidden() {
        let r = use_tasks_v2_with_collapse_effect(None, ExpandedView::Tasks);
        assert_eq!(r, CollapseDecision::CollapseToNone);
    }

    #[test]
    fn no_op_when_not_in_tasks_view() {
        let r = use_tasks_v2_with_collapse_effect(None, ExpandedView::Teammates);
        assert_eq!(r, CollapseDecision::KeepExpanded);
    }
}
