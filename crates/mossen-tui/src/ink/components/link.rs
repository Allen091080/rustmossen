//! Link component — OSC 8 hyperlinks (Link.tsx).

#[derive(Debug, Clone)]
pub struct LinkState {
    pub url: String,
    pub text: String,
    pub fallback_text: Option<String>,
}
impl LinkState {
    pub fn new(url: &str, text: &str) -> Self { Self { url: url.to_string(), text: text.to_string(), fallback_text: None } }
    pub fn render(&self, supports_hyperlinks: bool) -> String {
        if supports_hyperlinks {
            format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", self.url, self.text)
        } else {
            self.fallback_text.as_deref().unwrap_or(&self.text).to_string()
        }
    }
}

/// TS `Link` exports `type Props`.
pub type Props = LinkState;
