//! OAuth service — OAuth 2.0 authorization code flow with PKCE.

pub mod crypto;
pub mod auth_code_listener;
pub mod client;
pub mod profile;
pub mod service;

pub use service::OAuthService;
pub use client::{
    build_auth_url, exchange_code_for_tokens, fetch_profile_info, is_oauth_token_expired,
    refresh_oauth_token, should_use_hosted_auth, parse_scopes,
};
pub use crypto::{generate_code_challenge, generate_code_verifier, generate_state};
pub use profile::{get_oauth_profile_from_api_key, get_oauth_profile_from_oauth_token};
