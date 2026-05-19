//! Check if terminal supports hyperlinks (supports-hyperlinks.ts).

/// Check if the current terminal supports OSC 8 hyperlinks.
pub fn supports_hyperlinks() -> bool {
    // Check common terminal emulators that support hyperlinks
    if let Ok(term) = std::env::var("TERM_PROGRAM") {
        return matches!(term.as_str(), "iTerm.app" | "WezTerm" | "vscode" | "Hyper" | "Alacritty");
    }
    if std::env::var("FORCE_HYPERLINK").ok().as_deref() == Some("1") { return true; }
    if std::env::var("VTE_VERSION").is_ok() { return true; }
    false
}

/// Additional terminal program names known to support OSC 8 hyperlinks.
pub const ADDITIONAL_HYPERLINK_TERMINALS: &[&str] = &[
    "kitty", "ghostty", "Konsole", "Terminator", "tilix", "rio",
];
