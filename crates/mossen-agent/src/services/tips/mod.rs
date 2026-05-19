//! Tips service — contextual tips shown to users during spinner/loading states.

pub mod history;
pub mod registry;
pub mod scheduler;

pub use history::{record_tip_shown_in_history, get_sessions_since_last_shown};
pub use registry::{get_relevant_tips, Tip, TipContext};
pub use scheduler::{get_tip_to_show_on_spinner, select_tip_with_longest_time_since_shown, record_shown_tip};
