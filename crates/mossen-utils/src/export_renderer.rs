//! Streaming message-export rendering.
//!
//! Mirrors TS `utils/exportRenderer.tsx`. The TS version takes a list of
//! `Message`s plus a tools array, renders them in chunks through React/Ink,
//! and emits the ANSI output via a sink callback. The Rust port keeps the
//! chunking + sink protocol but lets the caller supply the per-chunk
//! renderer closure (since the actual rendering lives in the front-end
//! crate, not in mossen-utils).

use crate::static_render::strip_ansi;

/// Upper bound on how many normalized messages a single `Message` can
/// produce. In TS this delegates to `normalizeMessages` which splits each
/// message into one normalized entry per content block (≥ 1). The Rust
/// port accepts a `block_counts` slice from the caller so we don't need to
/// pull the normalize pipeline in here.
pub fn normalized_upper_bound(block_count: usize) -> usize {
    block_count.max(1)
}

/// Compute the iteration ceiling for the chunked render loop.
///
/// Sums the per-message upper bounds and adds `chunk_size` so the loop
/// always reaches the empty slice where it can short-circuit.
pub fn render_ceiling(block_counts: &[usize], chunk_size: usize) -> usize {
    let mut ceiling = chunk_size;
    for &bc in block_counts {
        ceiling += normalized_upper_bound(bc);
    }
    ceiling
}

/// Options for streaming message rendering.
#[derive(Debug, Clone)]
pub struct StreamRenderOptions {
    pub columns: Option<u16>,
    pub verbose: bool,
    pub chunk_size: usize,
}

impl Default for StreamRenderOptions {
    fn default() -> Self {
        Self {
            columns: None,
            verbose: false,
            chunk_size: 40,
        }
    }
}

/// Streams rendered messages in chunks. Each chunk is rendered via the
/// caller-supplied `render_chunk` closure (which knows how to materialize
/// the message range to ANSI text). The `sink` callback receives each
/// chunk's ANSI bytes — write it to stdout, append to a file, etc.
///
/// `block_counts[i]` is the upper bound on normalized entries produced
/// by message `i`. Pass `&[1; N]` if you don't have block-level info.
///
/// `on_progress`, when set, fires with the running `rendered` count after
/// each non-empty chunk — mirroring the TS callback.
pub async fn stream_rendered_messages<R, RFut, S, SFut, P>(
    block_counts: &[usize],
    options: &StreamRenderOptions,
    mut render_chunk: R,
    mut sink: S,
    mut on_progress: Option<P>,
) where
    R: FnMut(usize, usize) -> RFut,
    RFut: std::future::Future<Output = String>,
    S: FnMut(String) -> SFut,
    SFut: std::future::Future<Output = ()>,
    P: FnMut(usize),
{
    let chunk_size = options.chunk_size.max(1);
    let ceiling = render_ceiling(block_counts, chunk_size);

    let mut offset = 0usize;
    while offset < ceiling {
        let end = offset + chunk_size;
        let ansi = render_chunk(offset, end).await;
        if strip_ansi(&ansi).trim().is_empty() {
            break;
        }
        sink(ansi).await;
        if let Some(cb) = on_progress.as_mut() {
            cb(end);
        }
        offset = end;
    }
}

/// Renders all messages to a single plain-text string. Mirrors TS
/// `renderMessagesToPlainText`. Internally calls
/// [`stream_rendered_messages`] and concatenates ANSI-stripped chunks.
pub async fn render_messages_to_plain_text<R, RFut>(
    block_counts: &[usize],
    columns: Option<u16>,
    render_chunk: R,
) -> String
where
    R: FnMut(usize, usize) -> RFut,
    RFut: std::future::Future<Output = String>,
{
    let opts = StreamRenderOptions {
        columns,
        ..Default::default()
    };
    let parts = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let parts_clone = std::sync::Arc::clone(&parts);
    stream_rendered_messages::<_, _, _, _, fn(usize)>(
        block_counts,
        &opts,
        render_chunk,
        move |chunk| {
            let parts = std::sync::Arc::clone(&parts_clone);
            async move {
                parts.lock().unwrap().push(strip_ansi(&chunk));
            }
        },
        None,
    )
    .await;

    let guard = parts.lock().unwrap();
    guard.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceiling_includes_chunk_size_and_blocks() {
        assert_eq!(render_ceiling(&[1, 1, 1], 40), 43);
        assert_eq!(render_ceiling(&[2, 3], 10), 15);
    }

    #[test]
    fn upper_bound_min_one() {
        assert_eq!(normalized_upper_bound(0), 1);
        assert_eq!(normalized_upper_bound(5), 5);
    }
}
