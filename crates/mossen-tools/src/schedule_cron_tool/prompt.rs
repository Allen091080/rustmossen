//! ScheduleCronTool prompt and constants.
//!
//! Translated from tools/ScheduleCronTool/prompt.ts

/// Default max age in days for recurring tasks before auto-expiry.
pub const DEFAULT_MAX_AGE_DAYS: u64 = 7;

pub const CRON_CREATE_TOOL_NAME: &str = "CronCreate";
pub const CRON_DELETE_TOOL_NAME: &str = "CronDelete";
pub const CRON_LIST_TOOL_NAME: &str = "CronList";

/// Gate for the cron scheduling system.
/// Returns true unless MOSSEN_CODE_DISABLE_CRON is set.
pub fn is_kairos_cron_enabled() -> bool {
    std::env::var("MOSSEN_CODE_DISABLE_CRON")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
        == false
}

/// Kill switch for disk-persistent (durable) cron tasks.
pub fn is_durable_cron_enabled() -> bool {
    // Default true; can be overridden by feature flag at runtime
    true
}

pub fn build_cron_create_description(durable_enabled: bool) -> &'static str {
    if durable_enabled {
        "Schedule a prompt to run at a future time \u{2014} either recurring on a cron schedule, or once at a specific time. Pass durable: true to persist to .mossen/scheduled_tasks.json; otherwise session-only."
    } else {
        "Schedule a prompt to run at a future time within this Mossen session \u{2014} either recurring on a cron schedule, or once at a specific time."
    }
}

pub fn build_cron_create_prompt(durable_enabled: bool) -> String {
    let durability_section = if durable_enabled {
        format!(
            "## Durability\n\n\
             By default (durable: false) the job lives only in this Mossen session \u{2014} nothing is written to disk, and the job is gone when Mossen exits. \
             Pass durable: true to write to .mossen/scheduled_tasks.json so the job survives restarts. \
             Only use durable: true when the user explicitly asks for the task to persist (\"keep doing this every day\", \"set this up permanently\"). \
             Most \"remind me in 5 minutes\" / \"check back in an hour\" requests should stay session-only."
        )
    } else {
        "## Session-only\n\n\
         Jobs live only in this Mossen session \u{2014} nothing is written to disk, and the job is gone when Mossen exits."
            .to_string()
    };

    let durable_runtime_note = if durable_enabled {
        "Durable jobs persist to .mossen/scheduled_tasks.json and survive session restarts \u{2014} on next launch they resume automatically. One-shot durable tasks that were missed while the REPL was closed are surfaced for catch-up. Session-only jobs die with the process. "
    } else {
        ""
    };

    format!(
        r#"Schedule a prompt to be enqueued at a future time. Use for both recurring schedules and one-shot reminders.

Uses standard 5-field cron in the user's local timezone: minute hour day-of-month month day-of-week. "0 9 * * *" means 9am local — no timezone conversion needed.

## One-shot tasks (recurring: false)

For "remind me at X" or "at <time>, do Y" requests — fire once then auto-delete.
Pin minute/hour/day-of-month/month to specific values:
  "remind me at 2:30pm today to check the deploy" → cron: "30 14 <today_dom> <today_month> *", recurring: false
  "tomorrow morning, run the smoke test" → cron: "57 8 <tomorrow_dom> <tomorrow_month> *", recurring: false

## Recurring jobs (recurring: true, the default)

For "every N minutes" / "every hour" / "weekdays at 9am" requests:
  "*/5 * * * *" (every 5 min), "0 * * * *" (hourly), "0 9 * * 1-5" (weekdays at 9am local)

## Avoid the :00 and :30 minute marks when the task allows it

Every user who asks for "9am" gets `0 9`, and every user who asks for "hourly" gets `0 *` — which means requests from across the planet land on the API at the same instant. When the user's request is approximate, pick a minute that is NOT 0 or 30:
  "every morning around 9" → "57 8 * * *" or "3 9 * * *" (not "0 9 * * *")
  "hourly" → "7 * * * *" (not "0 * * * *")
  "in an hour or so, remind me to..." → pick whatever minute you land on, don't round

Only use minute 0 or 30 when the user names that exact time and clearly means it ("at 9:00 sharp", "at half past", coordinating with a meeting). When in doubt, nudge a few minutes early or late — the user will not notice, and the fleet will.

{durability_section}

## Runtime behavior

Jobs only fire while the REPL is idle (not mid-query). {durable_runtime_note}The scheduler adds a small deterministic jitter on top of whatever you pick: recurring tasks fire up to 10% of their period late (max 15 min); one-shot tasks landing on :00 or :30 fire up to 90 s early. Picking an off-minute is still the bigger lever.

Recurring tasks auto-expire after {max_age} days — they fire one final time, then are deleted. This bounds session lifetime. Tell the user about the {max_age}-day limit when scheduling recurring jobs.

Returns a job ID you can pass to {cron_delete}."#,
        durability_section = durability_section,
        durable_runtime_note = durable_runtime_note,
        max_age = DEFAULT_MAX_AGE_DAYS,
        cron_delete = CRON_DELETE_TOOL_NAME
    )
}

pub const CRON_DELETE_DESCRIPTION: &str = "Cancel a scheduled cron job by ID";

pub fn build_cron_delete_prompt(durable_enabled: bool) -> String {
    if durable_enabled {
        format!(
            "Cancel a cron job previously scheduled with {}. Removes it from .mossen/scheduled_tasks.json (durable jobs) or the in-memory session store (session-only jobs).",
            CRON_CREATE_TOOL_NAME
        )
    } else {
        format!(
            "Cancel a cron job previously scheduled with {}. Removes it from the in-memory session store.",
            CRON_CREATE_TOOL_NAME
        )
    }
}

pub const CRON_LIST_DESCRIPTION: &str = "List scheduled cron jobs";

pub fn build_cron_list_prompt(durable_enabled: bool) -> String {
    if durable_enabled {
        format!(
            "List all cron jobs scheduled via {}, both durable (.mossen/scheduled_tasks.json) and session-only.",
            CRON_CREATE_TOOL_NAME
        )
    } else {
        format!(
            "List all cron jobs scheduled via {} in this session.",
            CRON_CREATE_TOOL_NAME
        )
    }
}
