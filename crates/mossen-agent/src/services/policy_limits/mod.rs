//! Policy limits service — fetches org-level policy restrictions from the API.
//!
//! Follows fail-open pattern: if fetch fails, continues without restrictions.
//! Uses ETag caching, background polling (1 hour interval), retry logic.

pub mod service;
pub mod types;

pub use service::{
    clear_policy_limits_cache, initialize_policy_limits_loading_promise, is_policy_allowed,
    is_policy_limits_eligible, load_policy_limits, refresh_policy_limits, start_background_polling,
    stop_background_polling, wait_for_policy_limits_to_load,
};
pub use types::{PolicyLimitsFetchResult, PolicyLimitsResponse};
