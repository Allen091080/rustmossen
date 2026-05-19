//! Auth code listener — temporary localhost HTTP server for OAuth redirect capture.

use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Temporary localhost HTTP server that captures OAuth authorization code redirects.
pub struct AuthCodeListener {
    port: u16,
    shutdown_tx: Option<oneshot::Sender<()>>,
    pending_response: Arc<tokio::sync::Mutex<bool>>,
}

impl AuthCodeListener {
    pub fn new() -> Self {
        Self {
            port: 0,
            shutdown_tx: None,
            pending_response: Arc::new(tokio::sync::Mutex::new(false)),
        }
    }

    /// Start listening on an OS-assigned port. Returns the port number.
    pub async fn start(&mut self) -> Result<u16, std::io::Error> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        self.port = addr.port();

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(shutdown_tx);

        let pending = Arc::clone(&self.pending_response);

        // Spawn the server task
        tokio::spawn(async move {
            let _listener = listener;
            let _pending = pending;
            let _shutdown_rx = shutdown_rx;
            // Server loop handled by wait_for_authorization
        });

        Ok(self.port)
    }

    pub fn get_port(&self) -> u16 {
        self.port
    }

    pub fn has_pending_response(&self) -> bool {
        *self.pending_response.blocking_lock()
    }

    /// Wait for the authorization code callback.
    pub async fn wait_for_authorization(
        &self,
        expected_state: &str,
        callback_path: &str,
    ) -> Result<String, String> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port))
            .await
            .map_err(|e| format!("Failed to bind: {}", e))?;

        let pending = Arc::clone(&self.pending_response);
        let expected_state = expected_state.to_string();
        let callback_path = callback_path.to_string();

        loop {
            let (stream, _) = listener
                .accept()
                .await
                .map_err(|e| format!("Accept failed: {}", e))?;

            let mut buf = vec![0u8; 4096];
            use tokio::io::AsyncReadExt;
            let n = stream
                .readable()
                .await
                .map_err(|e| format!("Read failed: {}", e))?;
            let _ = n;
            let stream_ref = &stream;
            let n = stream_ref
                .try_read(&mut buf)
                .unwrap_or(0);

            let request = String::from_utf8_lossy(&buf[..n]);
            let first_line = request.lines().next().unwrap_or("");

            // Parse GET /callback?code=XXX&state=YYY HTTP/1.1
            if !first_line.contains(&callback_path) {
                use tokio::io::AsyncWriteExt;
                let response = "HTTP/1.1 404 Not Found\r\n\r\n";
                let _ = stream.try_write(response.as_bytes());
                continue;
            }

            // Extract query parameters
            let path_and_query = first_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("");
            let url = format!("http://localhost{}", path_and_query);
            let parsed = url::Url::parse(&url).map_err(|e| format!("URL parse: {}", e))?;

            let code = parsed
                .query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.to_string());
            let state = parsed
                .query_pairs()
                .find(|(k, _)| k == "state")
                .map(|(_, v)| v.to_string());

            if code.is_none() {
                use tokio::io::AsyncWriteExt;
                let response = "HTTP/1.1 400 Bad Request\r\n\r\nAuthorization code not found";
                let _ = stream.try_write(response.as_bytes());
                return Err("No authorization code received".to_string());
            }

            if state.as_deref() != Some(&expected_state) {
                use tokio::io::AsyncWriteExt;
                let response = "HTTP/1.1 400 Bad Request\r\n\r\nInvalid state parameter";
                let _ = stream.try_write(response.as_bytes());
                return Err("Invalid state parameter".to_string());
            }

            *pending.lock().await = true;

            // Send success response
            use tokio::io::AsyncWriteExt;
            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Authorization successful</h1><p>You can close this window.</p></body></html>";
            let _ = stream.try_write(response.as_bytes());

            return Ok(code.unwrap());
        }
    }

    /// Close the listener and clean up.
    pub fn close(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Default for AuthCodeListener {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AuthCodeListener {
    fn drop(&mut self) {
        self.close();
    }
}
