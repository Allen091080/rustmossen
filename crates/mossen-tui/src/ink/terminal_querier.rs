//! Terminal querier (terminal-querier.ts).
//!
//! Builds outbound terminal queries (DECRQM, DA1/DA2, Kitty flags, OSC colour,
//! XTVERSION, cursor position) and provides a queue that pairs each request
//! with the expected inbound response.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::ink::parse_keypress::TerminalResponse;

/// A query is a request string + a matcher closure that recognises the reply.
pub struct TerminalQuery {
    pub request: String,
    pub matcher: Box<dyn Fn(&TerminalResponse) -> bool + Send + Sync + 'static>,
}

impl std::fmt::Debug for TerminalQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalQuery")
            .field("request", &self.request)
            .finish()
    }
}

fn csi(s: &str) -> String {
    format!("\x1b[{}", s)
}

fn osc(code: u32, body: &str) -> String {
    format!("\x1b]{};{}{}", code, body, "\x07")
}

/// DECRQM: request DEC private mode status (CSI ? mode $ p).
pub fn decrqm(mode: u32) -> TerminalQuery {
    let want = mode;
    TerminalQuery {
        request: csi(&format!("?{}$p", mode)),
        matcher: Box::new(move |r| matches!(r, TerminalResponse::Decrpm { mode, .. } if *mode == want)),
    }
}

/// Primary Device Attributes (CSI c).
pub fn da1() -> TerminalQuery {
    TerminalQuery {
        request: csi("c"),
        matcher: Box::new(|r| matches!(r, TerminalResponse::Da1 { .. })),
    }
}

/// Secondary Device Attributes (CSI > c).
pub fn da2() -> TerminalQuery {
    TerminalQuery {
        request: csi(">c"),
        matcher: Box::new(|r| matches!(r, TerminalResponse::Da2 { .. })),
    }
}

/// Kitty keyboard flags (CSI ? u).
pub fn kitty_keyboard() -> TerminalQuery {
    TerminalQuery {
        request: csi("?u"),
        matcher: Box::new(|r| matches!(r, TerminalResponse::KittyKeyboard { .. })),
    }
}

/// DECXCPR cursor position (CSI ? 6 n).
pub fn cursor_position() -> TerminalQuery {
    TerminalQuery {
        request: csi("?6n"),
        matcher: Box::new(|r| matches!(r, TerminalResponse::CursorPosition { .. })),
    }
}

/// OSC dynamic colour query (e.g. OSC 11 for background).
pub fn osc_color(code: u32) -> TerminalQuery {
    let want = code;
    TerminalQuery {
        request: osc(code, "?"),
        matcher: Box::new(move |r| matches!(r, TerminalResponse::Osc { code, .. } if *code == want)),
    }
}

/// XTVERSION (CSI > 0 q).
pub fn xtversion() -> TerminalQuery {
    TerminalQuery {
        request: csi(">0q"),
        matcher: Box::new(|r| matches!(r, TerminalResponse::Xtversion { .. })),
    }
}

const SENTINEL: &str = "\x1b[c";

enum Pending {
    Query {
        matcher: Box<dyn Fn(&TerminalResponse) -> bool + Send + Sync + 'static>,
        resolved: Arc<Mutex<Option<TerminalResponse>>>,
    },
    Sentinel(Arc<Mutex<bool>>),
}

/// Queue of in-flight queries pending terminal replies.
pub struct TerminalQuerier {
    queue: Mutex<VecDeque<Pending>>,
    pub stdout: Mutex<String>,
}

impl TerminalQuerier {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            stdout: Mutex::new(String::new()),
        }
    }

    /// Send a query and return a handle that resolves when the matching
    /// response or sentinel arrives.
    pub fn send(&self, query: TerminalQuery) -> Arc<Mutex<Option<TerminalResponse>>> {
        let resolved = Arc::new(Mutex::new(None));
        if let Ok(mut buf) = self.stdout.lock() {
            buf.push_str(&query.request);
        }
        if let Ok(mut q) = self.queue.lock() {
            q.push_back(Pending::Query {
                matcher: query.matcher,
                resolved: resolved.clone(),
            });
        }
        resolved
    }

    /// Send a DA1 sentinel; the returned handle goes `true` when received.
    pub fn flush(&self) -> Arc<Mutex<bool>> {
        let done = Arc::new(Mutex::new(false));
        if let Ok(mut buf) = self.stdout.lock() {
            buf.push_str(SENTINEL);
        }
        if let Ok(mut q) = self.queue.lock() {
            q.push_back(Pending::Sentinel(done.clone()));
        }
        done
    }

    /// Feed a response from stdin; resolves the next matching pending query.
    pub fn handle_response(&self, response: TerminalResponse) {
        let Ok(mut q) = self.queue.lock() else { return };
        // Walk the queue and resolve the first matching query. If we hit a
        // sentinel, mark unsupported responses for any queries queued before
        // it.
        let mut idx = 0;
        let mut resolved_idx: Option<usize> = None;
        while idx < q.len() {
            match &q[idx] {
                Pending::Query { matcher, .. } => {
                    if matcher(&response) {
                        resolved_idx = Some(idx);
                        break;
                    }
                }
                Pending::Sentinel(_) => {
                    if matches!(response, TerminalResponse::Da1 { .. }) {
                        resolved_idx = Some(idx);
                        break;
                    }
                }
            }
            idx += 1;
        }
        if let Some(i) = resolved_idx {
            match q.remove(i).unwrap() {
                Pending::Query { resolved, .. } => {
                    if let Ok(mut slot) = resolved.lock() {
                        *slot = Some(response);
                    }
                }
                Pending::Sentinel(done) => {
                    if let Ok(mut d) = done.lock() {
                        *d = true;
                    }
                }
            }
        }
    }
}

impl Default for TerminalQuerier {
    fn default() -> Self { Self::new() }
}
