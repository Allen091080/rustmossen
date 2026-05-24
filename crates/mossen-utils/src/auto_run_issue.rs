//! Auto-run /issue command — feedback survey follow-up logic.
//!
//! Mirrors TS `utils/autoRunIssue.tsx`. The TSX file exports a React
//! notification component (`AutoRunIssueNotification`) plus three pure
//! helper functions. The Rust port omits JSX and translates the helpers.

/// Reason the auto-run /issue feature is being triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoRunIssueReason {
    FeedbackSurveyBad,
    FeedbackSurveyGood,
}

impl AutoRunIssueReason {
    /// Parse a stringly-typed reason (matches TS string-literal type).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "feedback_survey_bad" => Some(Self::FeedbackSurveyBad),
            "feedback_survey_good" => Some(Self::FeedbackSurveyGood),
            _ => None,
        }
    }

    /// String form (matches TS string-literal value).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FeedbackSurveyBad => "feedback_survey_bad",
            Self::FeedbackSurveyGood => "feedback_survey_good",
        }
    }
}

/// Build flavor — `"internal"` for internal builds, anything else (typically
/// `"external"`) for public builds. The TS code compares against the
/// literal `"external"` after a cast; the Rust port keeps the same gating
/// signal via `USER_TYPE` env var, which is how the running flavor is
/// already plumbed elsewhere.
fn is_internal_build() -> bool {
    std::env::var("USER_TYPE")
        .map(|v| v == "internal")
        .unwrap_or(false)
}

/// Determines if `/issue` should auto-run for the given feedback reason.
///
/// Currently returns `false` in all cases — matching the TS implementation
/// which has every arm of the switch returning `false`. The hook is kept
/// in place so build flavors that flip this on can do so by editing the
/// table below rather than searching for call sites.
pub fn should_auto_run_issue(reason: AutoRunIssueReason) -> bool {
    if !is_internal_build() {
        return false;
    }
    match reason {
        AutoRunIssueReason::FeedbackSurveyBad => false,
        AutoRunIssueReason::FeedbackSurveyGood => false,
    }
}

/// Returns the slash command to auto-run for the given reason.
///
/// Internal builds get `/good-mossen` for "good" feedback; everything else
/// (and "bad" feedback in any build) gets `/issue`.
pub fn get_auto_run_command(reason: AutoRunIssueReason) -> &'static str {
    if is_internal_build() && matches!(reason, AutoRunIssueReason::FeedbackSurveyGood) {
        "/good-mossen"
    } else {
        "/issue"
    }
}

/// Human-readable description of why `/issue` is being auto-run.
pub fn get_auto_run_issue_reason_text(reason: AutoRunIssueReason) -> &'static str {
    match reason {
        AutoRunIssueReason::FeedbackSurveyBad => "You responded \"Bad\" to the feedback survey",
        AutoRunIssueReason::FeedbackSurveyGood => "You responded \"Good\" to the feedback survey",
    }
}

/// React-tied notification component reduced to its logic core.
///
/// 在 TS 中 `AutoRunIssueNotification` 是 React 组件：它在挂载后调用 `onRun`
/// 一次、并允许 ESC 取消。Rust 端无 React，因此暴露一个等价的“辅助函数”，
/// 调用者只需在收到 ESC 时调用 `cancel`，未取消则调用 `run` 一次。这里把它
/// 显式化为一个状态机句柄，方便上层封装 UI。
pub struct AutoRunIssueNotification {
    has_run: bool,
    reason: String,
}

impl AutoRunIssueNotification {
    /// 构造组件等价的状态。`reason` 直接对应 React props。
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            has_run: false,
            reason: reason.into(),
        }
    }

    /// 模拟 `useEffect` — 第一次调用执行 `on_run`，后续调用为 no-op。
    pub fn run_if_needed<F: FnOnce()>(&mut self, on_run: F) {
        if !self.has_run {
            self.has_run = true;
            on_run();
        }
    }

    /// 模拟 `useKeybinding('confirm:no', onCancel)` —— 取消时调用。
    pub fn cancel<F: FnOnce()>(&self, on_cancel: F) {
        on_cancel();
    }

    /// 透出给 UI 层的展示理由文本。
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip() {
        for s in ["feedback_survey_bad", "feedback_survey_good"] {
            let parsed = AutoRunIssueReason::parse(s).unwrap();
            assert_eq!(parsed.as_str(), s);
        }
        assert!(AutoRunIssueReason::parse("nope").is_none());
    }

    #[test]
    fn command_defaults_to_issue() {
        // Don't depend on USER_TYPE — "bad" always returns /issue.
        assert_eq!(
            get_auto_run_command(AutoRunIssueReason::FeedbackSurveyBad),
            "/issue"
        );
    }

    #[test]
    fn reason_text_distinct() {
        assert_ne!(
            get_auto_run_issue_reason_text(AutoRunIssueReason::FeedbackSurveyBad),
            get_auto_run_issue_reason_text(AutoRunIssueReason::FeedbackSurveyGood),
        );
    }
}
