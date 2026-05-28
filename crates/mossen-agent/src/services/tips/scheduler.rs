//! Tip scheduler — selects which tip to show based on recency and context.

use super::history::{get_sessions_since_last_shown, record_tip_shown_in_history};
use super::registry::{get_relevant_tips, Tip, TipContext};

/// Select the tip that hasn't been shown for the longest time.
pub fn select_tip_with_longest_time_since_shown(available_tips: &[Tip]) -> Option<&Tip> {
    if available_tips.is_empty() {
        return None;
    }
    if available_tips.len() == 1 {
        return Some(&available_tips[0]);
    }

    available_tips
        .iter()
        .max_by_key(|tip| get_sessions_since_last_shown(&tip.id))
}

/// Get the tip to show on spinner, respecting cooldown and settings.
pub async fn get_tip_to_show_on_spinner(context: Option<&TipContext>) -> Option<Tip> {
    let tips = get_relevant_tips(context).await;
    if tips.is_empty() {
        return None;
    }

    select_tip_with_longest_time_since_shown(&tips).cloned()
}

/// Record that a tip was shown (updates history and logs analytics).
pub fn record_shown_tip(tip: &Tip) {
    record_tip_shown_in_history(&tip.id);
}
