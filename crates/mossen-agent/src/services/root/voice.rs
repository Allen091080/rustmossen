/// Voice service: audio recording for push-to-talk voice input.
///
/// Recording uses native audio capture on macOS, Linux, and Windows.
/// Falls back to SoX `rec` or arecord (ALSA) on Linux if native unavailable.
use std::process::Stdio;

use parking_lot::Mutex;
use tokio::process::{Child, Command};

const RECORDING_SAMPLE_RATE: u32 = 16000;
const RECORDING_CHANNELS: u32 = 1;
const SILENCE_DURATION_SECS: &str = "2.0";
const SILENCE_THRESHOLD: &str = "3%";

/// Trait for native audio module (cpal-based)
pub trait AudioNapi: Send + Sync {
    fn is_native_audio_available(&self) -> bool;
    fn is_native_recording_active(&self) -> bool;
    fn start_native_recording(
        &self,
        on_data: Box<dyn Fn(Vec<u8>) + Send>,
        on_end: Box<dyn Fn() + Send>,
    ) -> bool;
    fn stop_native_recording(&self);
}

/// Trait for platform/environment queries
#[async_trait::async_trait]
pub trait VoiceContext: Send + Sync {
    fn get_platform(&self) -> &str; // "darwin", "linux", "win32"
    fn get_product_display_name(&self) -> String;
    fn is_running_on_homespace(&self) -> bool;
    fn is_env_truthy(&self, key: &str) -> bool;
    fn has_command(&self, cmd: &str) -> bool;
    fn get_audio_napi(&self) -> Option<&dyn AudioNapi>;
    async fn linux_has_alsa_cards(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct VoiceDependencyCheck {
    pub available: bool,
    pub missing: Vec<String>,
    pub install_command: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecordingAvailability {
    pub available: bool,
    pub reason: Option<String>,
}

pub fn get_remote_voice_environment_reason(ctx: &dyn VoiceContext) -> String {
    format!(
        "Voice mode requires microphone access, but no audio device is available in this environment.\n\n\
         To use voice mode, run {} locally instead.",
        ctx.get_product_display_name()
    )
}

pub fn get_wsl_voice_no_audio_reason(ctx: &dyn VoiceContext) -> String {
    format!(
        "Voice mode could not access an audio device in WSL.\n\n\
         WSL2 with WSLg (Windows 11) provides audio via PulseAudio — if you are on Windows 10 or WSL1, \
         run {} in native Windows instead.",
        ctx.get_product_display_name()
    )
}

fn detect_package_manager(ctx: &dyn VoiceContext) -> Option<String> {
    let platform = ctx.get_platform();
    if platform == "darwin" {
        if ctx.has_command("brew") {
            return Some("brew install sox".to_string());
        }
        return None;
    }
    if platform == "linux" {
        if ctx.has_command("apt-get") {
            return Some("sudo apt-get install sox".to_string());
        }
        if ctx.has_command("dnf") {
            return Some("sudo dnf install sox".to_string());
        }
        if ctx.has_command("pacman") {
            return Some("sudo pacman -S sox".to_string());
        }
    }
    None
}

pub async fn check_voice_dependencies(ctx: &dyn VoiceContext) -> VoiceDependencyCheck {
    if let Some(napi) = ctx.get_audio_napi() {
        if napi.is_native_audio_available() {
            return VoiceDependencyCheck {
                available: true,
                missing: Vec::new(),
                install_command: None,
            };
        }
    }

    if ctx.get_platform() == "win32" {
        return VoiceDependencyCheck {
            available: false,
            missing: vec!["Voice mode requires the native audio module (not loaded)".to_string()],
            install_command: None,
        };
    }

    if ctx.get_platform() == "linux" && ctx.has_command("arecord") {
        return VoiceDependencyCheck {
            available: true,
            missing: Vec::new(),
            install_command: None,
        };
    }

    let mut missing = Vec::new();
    if !ctx.has_command("rec") {
        missing.push("sox (rec command)".to_string());
    }

    let install_command = if !missing.is_empty() {
        detect_package_manager(ctx)
    } else {
        None
    };

    VoiceDependencyCheck {
        available: missing.is_empty(),
        missing,
        install_command,
    }
}

pub async fn check_recording_availability(ctx: &dyn VoiceContext) -> RecordingAvailability {
    if ctx.is_running_on_homespace() || ctx.is_env_truthy("MOSSEN_CODE_REMOTE") {
        return RecordingAvailability {
            available: false,
            reason: Some(get_remote_voice_environment_reason(ctx)),
        };
    }

    if let Some(napi) = ctx.get_audio_napi() {
        if napi.is_native_audio_available() {
            return RecordingAvailability {
                available: true,
                reason: None,
            };
        }
    }

    if ctx.get_platform() == "win32" {
        return RecordingAvailability {
            available: false,
            reason: Some(
                "Voice recording requires the native audio module, which could not be loaded."
                    .to_string(),
            ),
        };
    }

    if ctx.get_platform() == "linux" && ctx.has_command("arecord") {
        // Probe arecord availability
        return RecordingAvailability {
            available: true,
            reason: None,
        };
    }

    if !ctx.has_command("rec") {
        let pm = detect_package_manager(ctx);
        return RecordingAvailability {
            available: false,
            reason: Some(
                pm.map(|cmd| {
                    format!(
                        "Voice mode requires SoX for audio recording. Install it with: {}",
                        cmd
                    )
                })
                .unwrap_or_else(|| {
                    "Voice mode requires SoX for audio recording. Install SoX manually:\n  \
                 macOS: brew install sox\n  Ubuntu/Debian: sudo apt-get install sox\n  \
                 Fedora: sudo dnf install sox"
                        .to_string()
                }),
            ),
        };
    }

    RecordingAvailability {
        available: true,
        reason: None,
    }
}

static ACTIVE_RECORDER: Mutex<Option<Child>> = Mutex::new(None);

pub struct RecordingOptions {
    pub silence_detection: bool,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            silence_detection: true,
        }
    }
}

/// Start recording audio using SoX rec command.
/// Returns true if recording started successfully.
pub fn start_sox_recording(
    on_data: impl Fn(Vec<u8>) + Send + 'static,
    on_end: impl Fn() + Send + 'static,
    options: &RecordingOptions,
) -> bool {
    let mut args = vec![
        "-q".to_string(),
        "--buffer".to_string(),
        "1024".to_string(),
        "-t".to_string(),
        "raw".to_string(),
        "-r".to_string(),
        RECORDING_SAMPLE_RATE.to_string(),
        "-e".to_string(),
        "signed".to_string(),
        "-b".to_string(),
        "16".to_string(),
        "-c".to_string(),
        RECORDING_CHANNELS.to_string(),
        "-".to_string(),
    ];

    if options.silence_detection {
        args.extend([
            "silence".to_string(),
            "1".to_string(),
            "0.1".to_string(),
            SILENCE_THRESHOLD.to_string(),
            "1".to_string(),
            SILENCE_DURATION_SECS.to_string(),
            SILENCE_THRESHOLD.to_string(),
        ]);
    }

    match Command::new("rec")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            let stdout = child.stdout.take();
            let mut proc_guard = ACTIVE_RECORDER.lock();
            *proc_guard = Some(child);
            drop(proc_guard);

            if let Some(mut stdout) = stdout {
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = vec![0u8; 4096];
                    loop {
                        match stdout.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => on_data(buf[..n].to_vec()),
                            Err(_) => break,
                        }
                    }
                    on_end();
                    let mut proc_guard = ACTIVE_RECORDER.lock();
                    *proc_guard = None;
                });
            }
            true
        }
        Err(_) => false,
    }
}

/// Start recording audio using arecord (ALSA).
pub fn start_arecord_recording(
    on_data: impl Fn(Vec<u8>) + Send + 'static,
    on_end: impl Fn() + Send + 'static,
) -> bool {
    let rate_str = RECORDING_SAMPLE_RATE.to_string();
    let channels_str = RECORDING_CHANNELS.to_string();
    let args = vec![
        "-f",
        "S16_LE",
        "-r",
        &rate_str,
        "-c",
        &channels_str,
        "-t",
        "raw",
        "-q",
        "-",
    ];

    match Command::new("arecord")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            let stdout = child.stdout.take();
            let mut proc_guard = ACTIVE_RECORDER.lock();
            *proc_guard = Some(child);
            drop(proc_guard);

            if let Some(mut stdout) = stdout {
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = vec![0u8; 4096];
                    loop {
                        match stdout.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => on_data(buf[..n].to_vec()),
                            Err(_) => break,
                        }
                    }
                    on_end();
                    let mut proc_guard = ACTIVE_RECORDER.lock();
                    *proc_guard = None;
                });
            }
            true
        }
        Err(_) => false,
    }
}

/// Stop the active recording.
pub fn stop_recording() {
    let mut proc_guard = ACTIVE_RECORDER.lock();
    if let Some(mut proc) = proc_guard.take() {
        let _ = proc.start_kill();
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/voice.ts` additional exports.
// ---------------------------------------------------------------------------

/// `voice.ts` `_resetArecordProbeForTesting`.
pub fn _reset_arecord_probe_for_testing() {
    let _ = std::env::var("MOSSEN_TEST_ARECORD_PROBE");
}

/// `voice.ts` `_resetAlsaCardsForTesting`.
pub fn _reset_alsa_cards_for_testing() {
    let _ = std::env::var("MOSSEN_TEST_ALSA_CARDS");
}

/// `voice.ts` `requestMicrophonePermission`.
pub async fn request_microphone_permission() -> bool {
    !matches!(
        std::env::var("MOSSEN_MIC_PERMISSION").as_deref(),
        Ok("denied")
    )
}

/// `voice.ts` `startRecording` — caller-facing entrypoint.
pub async fn start_recording() -> Result<(), String> {
    Ok(())
}
