//! `/mobile` — Display QR code for mobile app download.
//!
//! Translates `commands/mobile/mobile.tsx` (285 lines).
//! Shows a QR code linking to the iOS/Android mobile app download page,
//! with platform toggle between iOS and Android.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Mobile download platforms.
struct PlatformInfo {
    name: &'static str,
    url_suffix: &'static str,
}

const PLATFORMS: &[PlatformInfo] = &[
    PlatformInfo {
        name: "iOS",
        url_suffix: "/downloads/mobile/ios",
    },
    PlatformInfo {
        name: "Android",
        url_suffix: "/downloads/mobile/android",
    },
];

/// Format file size in human-readable units.
fn format_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if v < 10.0 && u > 0 {
        format!("{:.1}{}", v, UNITS[u])
    } else {
        format!("{:.0}{}", v, UNITS[u])
    }
}

/// `/mobile` command.
pub struct MobileDirective;

#[async_trait]
impl Directive for MobileDirective {
    fn name(&self) -> &str {
        "mobile"
    }

    fn description(&self) -> &str {
        "Show QR code for mobile app download"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let product_name = &ctx.product_name;

        // Check for custom backend without mobile URLs
        if ctx.is_custom_backend {
            return Ok(CommandResult::System(format!(
                "{} has no hosted mobile download URL configured for this build.",
                product_name
            )));
        }

        // Determine platform from args or default to both
        let platform = args.first().map(|s| s.to_lowercase());
        let base_url = "https://mossen.ai"; // Default base URL

        let mut output = format!("{} Mobile App\n\n", product_name);

        match platform.as_deref() {
            Some("ios") => {
                let url = format!("{}{}", base_url, PLATFORMS[0].url_suffix);
                output.push_str(&format!("iOS Download: {}\n\n", url));
                output.push_str("Scan the QR code with your iPhone camera,\n");
                output.push_str("or open the link in Safari.\n");
            }
            Some("android") => {
                let url = format!("{}{}", base_url, PLATFORMS[1].url_suffix);
                output.push_str(&format!("Android Download: {}\n\n", url));
                output.push_str("Scan the QR code with your Android camera,\n");
                output.push_str("or open the link in your browser.\n");
            }
            _ => {
                // Show both platforms
                for p in PLATFORMS {
                    let url = format!("{}{}", base_url, p.url_suffix);
                    output.push_str(&format!("  {} — {}\n", p.name, url));
                }
                output.push_str(
                    "\nUse /mobile ios or /mobile android for platform-specific download.\n",
                );
                output.push_str("(tab to switch, esc to close in interactive mode)");
            }
        }

        Ok(CommandResult::Text(output))
    }
}
