//! OAuth service — orchestrates the OAuth 2.0 authorization code flow with PKCE.

use super::auth_code_listener::AuthCodeListener;
use super::client::{self, OAuthConfig, OAuthTokenAccount, OAuthTokens};
use super::crypto;
use std::future::Future;
use std::pin::Pin;

/// OAuth service that handles the full authorization code flow with PKCE.
pub struct OAuthService {
    code_verifier: String,
    auth_code_listener: Option<AuthCodeListener>,
    port: Option<u16>,
}

/// Options for starting the OAuth flow.
pub struct OAuthFlowOptions {
    pub login_with_hosted_account: bool,
    pub inference_only: bool,
    pub expires_in: Option<u64>,
    pub org_uuid: Option<String>,
    pub login_hint: Option<String>,
    pub login_method: Option<String>,
    pub skip_browser_open: bool,
}

impl Default for OAuthFlowOptions {
    fn default() -> Self {
        Self {
            login_with_hosted_account: false,
            inference_only: false,
            expires_in: None,
            org_uuid: None,
            login_hint: None,
            login_method: None,
            skip_browser_open: false,
        }
    }
}

/// Type for the auth URL handler callback.
pub type AuthUrlHandler =
    Box<dyn FnOnce(String, Option<String>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

impl OAuthService {
    pub fn new() -> Self {
        Self {
            code_verifier: crypto::generate_code_verifier(),
            auth_code_listener: None,
            port: None,
        }
    }

    /// Start the OAuth flow.
    pub async fn start_oauth_flow(
        &mut self,
        config: &OAuthConfig,
        auth_url_handler: AuthUrlHandler,
        options: OAuthFlowOptions,
        open_browser: impl FnOnce(&str) -> Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Result<OAuthTokens, String> {
        // Create and start the auth code listener
        let mut listener = AuthCodeListener::new();
        let port = listener
            .start()
            .await
            .map_err(|e| format!("Failed to start OAuth callback server: {}", e))?;
        self.port = Some(port);

        // Generate PKCE values and state
        let code_challenge = crypto::generate_code_challenge(&self.code_verifier);
        let state = crypto::generate_state();

        // Build auth URLs
        let manual_url = client::build_auth_url(
            config,
            &code_challenge,
            &state,
            port,
            true,
            options.login_with_hosted_account,
            options.inference_only,
            options.org_uuid.as_deref(),
            options.login_hint.as_deref(),
            options.login_method.as_deref(),
        );
        let automatic_url = client::build_auth_url(
            config,
            &code_challenge,
            &state,
            port,
            false,
            options.login_with_hosted_account,
            options.inference_only,
            options.org_uuid.as_deref(),
            options.login_hint.as_deref(),
            options.login_method.as_deref(),
        );

        // Notify the handler and open browser
        if options.skip_browser_open {
            auth_url_handler(manual_url, Some(automatic_url)).await;
        } else {
            auth_url_handler(manual_url, None).await;
            open_browser(&automatic_url).await;
        }

        // Wait for authorization code
        let authorization_code = listener.wait_for_authorization(&state, "/callback").await?;

        // Exchange code for tokens
        let is_automatic = listener.has_pending_response();
        let token_response = client::exchange_code_for_tokens(
            config,
            &authorization_code,
            &state,
            &self.code_verifier,
            port,
            !is_automatic,
            options.expires_in,
        )
        .await?;

        // Fetch profile info
        let profile_info =
            client::fetch_profile_info(&config.base_api_url, &token_response.access_token).await?;

        let scopes = client::parse_scopes(token_response.scope.as_deref());
        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
            + token_response.expires_in * 1000;

        listener.close();
        self.auth_code_listener = None;

        Ok(OAuthTokens {
            access_token: token_response.access_token,
            refresh_token: token_response.refresh_token,
            expires_at,
            scopes,
            subscription_type: profile_info.subscription_type,
            rate_limit_tier: profile_info.rate_limit_tier,
            token_account: token_response.account.map(|a| OAuthTokenAccount {
                uuid: a.uuid,
                email_address: a.email_address,
                organization_uuid: token_response.organization.map(|o| o.uuid),
            }),
        })
    }

    /// Clean up resources.
    pub fn cleanup(&mut self) {
        if let Some(ref mut listener) = self.auth_code_listener {
            listener.close();
        }
        self.auth_code_listener = None;
    }
}

impl Default for OAuthService {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OAuthService {
    fn drop(&mut self) {
        self.cleanup();
    }
}
