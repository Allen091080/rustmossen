//! Voice state — reactive voice recording state store.
//!
//! Translates: context/voice.tsx
//! React context/provider → struct-based store with watch channel.

use std::sync::{Arc, RwLock};
use tokio::sync::watch;

/// Voice recording state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceRecordingState {
    Idle,
    Recording,
    Processing,
}

/// Complete voice state.
#[derive(Debug, Clone)]
pub struct VoiceState {
    pub voice_state: VoiceRecordingState,
    pub voice_error: Option<String>,
    pub voice_interim_transcript: String,
    pub voice_audio_levels: Vec<f32>,
    pub voice_warming_up: bool,
}

impl Default for VoiceState {
    fn default() -> Self {
        Self {
            voice_state: VoiceRecordingState::Idle,
            voice_error: None,
            voice_interim_transcript: String::new(),
            voice_audio_levels: Vec::new(),
            voice_warming_up: false,
        }
    }
}

/// Voice state store — thread-safe state with change notification.
#[derive(Debug, Clone)]
pub struct VoiceStateStore {
    state: Arc<RwLock<VoiceState>>,
    tx: Arc<watch::Sender<u64>>,
    rx: watch::Receiver<u64>,
    seq: Arc<std::sync::atomic::AtomicU64>,
}

impl VoiceStateStore {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(0);
        Self {
            state: Arc::new(RwLock::new(VoiceState::default())),
            tx: Arc::new(tx),
            rx,
            seq: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Get the current voice state.
    pub fn get_state(&self) -> VoiceState {
        self.state.read().unwrap().clone()
    }

    /// Set the voice state and notify subscribers.
    pub fn set_state(&self, updater: impl FnOnce(&mut VoiceState)) {
        let mut state = self.state.write().unwrap();
        updater(&mut state);
        drop(state);
        let _ = self
            .tx
            .send(self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    }

    /// Subscribe to state changes.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.rx.clone()
    }

    /// Select a slice of voice state.
    pub fn select<T: Clone>(&self, selector: impl Fn(&VoiceState) -> T) -> T {
        let state = self.state.read().unwrap();
        selector(&state)
    }
}

impl Default for VoiceStateStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `context/voice.tsx` exports.
// ---------------------------------------------------------------------------

use std::sync::OnceLock;

fn global_voice_store() -> &'static Arc<VoiceStateStore> {
    static G: OnceLock<Arc<VoiceStateStore>> = OnceLock::new();
    G.get_or_init(|| Arc::new(VoiceStateStore::new()))
}

/// `voice.tsx` `VoiceProvider`.
pub fn voice_provider() -> Arc<VoiceStateStore> {
    Arc::clone(global_voice_store())
}

/// `voice.tsx` `useVoiceState`.
pub fn use_voice_state() -> VoiceState {
    global_voice_store().get_state()
}

/// `voice.tsx` `useSetVoiceState`.
pub fn use_set_voice_state(new_state: VoiceState) {
    global_voice_store().set_state(|s| *s = new_state);
}

/// `voice.tsx` `useGetVoiceState`.
pub fn use_get_voice_state() -> VoiceState {
    global_voice_store().get_state()
}
