//! Layer 3 render layout caches.
//!
//! The semantic model owns what should be shown. This module caches terminal
//! layout facts derived from that model, so scrolling a long transcript does
//! not re-measure every block on every frame.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::render_model::RenderBlock;
use crate::render_profile::RendererProfile;
use crate::theme::ThemeName;

const DEFAULT_MAX_HEIGHT_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RenderHeightFlags {
    pub add_margin: bool,
    pub show_all_thinking: bool,
    pub focused: bool,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderHeightCacheStats {
    pub entries: usize,
    pub hits: usize,
    pub misses: usize,
    pub clears: usize,
}

#[derive(Debug)]
pub struct RenderHeightCache {
    max_entries: usize,
    entries: RefCell<HashMap<RenderHeightCacheKey, usize>>,
    hits: Cell<usize>,
    misses: Cell<usize>,
    clears: Cell<usize>,
}

impl Default for RenderHeightCache {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_HEIGHT_ENTRIES)
    }
}

impl RenderHeightCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries: max_entries.max(1),
            entries: RefCell::new(HashMap::new()),
            hits: Cell::new(0),
            misses: Cell::new(0),
            clears: Cell::new(0),
        }
    }

    pub fn height_for_block(
        &self,
        block: &RenderBlock,
        theme: ThemeName,
        width: u16,
        profile: RendererProfile,
        flags: RenderHeightFlags,
        compute: impl FnOnce() -> usize,
    ) -> usize {
        let key = RenderHeightCacheKey::for_block(block, theme, width, profile, flags);
        if let Some(height) = self.entries.borrow().get(&key).copied() {
            self.hits.set(self.hits.get().saturating_add(1));
            return height;
        }

        self.misses.set(self.misses.get().saturating_add(1));
        let height = compute();
        let mut entries = self.entries.borrow_mut();
        if entries.len() >= self.max_entries {
            entries.clear();
            self.clears.set(self.clears.get().saturating_add(1));
        }
        entries.insert(key, height);
        height
    }

    pub fn clear(&self) {
        self.entries.borrow_mut().clear();
        self.clears.set(self.clears.get().saturating_add(1));
    }

    pub fn stats(&self) -> RenderHeightCacheStats {
        RenderHeightCacheStats {
            entries: self.entries.borrow().len(),
            hits: self.hits.get(),
            misses: self.misses.get(),
            clears: self.clears.get(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RenderHeightCacheKey {
    block_id: String,
    block_signature: u64,
    theme: ThemeName,
    width: u16,
    profile: RendererProfile,
    flags: RenderHeightFlags,
}

impl RenderHeightCacheKey {
    fn for_block(
        block: &RenderBlock,
        theme: ThemeName,
        width: u16,
        profile: RendererProfile,
        flags: RenderHeightFlags,
    ) -> Self {
        Self {
            block_id: block.id.clone(),
            block_signature: block_signature(block),
            theme,
            width,
            profile,
            flags,
        }
    }
}

fn block_signature(block: &RenderBlock) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    block.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_model::{RenderBlockKind, RenderBlockState, RenderNode};

    fn block(content: &str, expanded: bool) -> RenderBlock {
        RenderBlock {
            id: "message-0".to_string(),
            source_indices: vec![0],
            kind: RenderBlockKind::Assistant,
            state: RenderBlockState {
                streaming: false,
                error: false,
                expanded,
            },
            nodes: vec![RenderNode::Markdown(content.to_string())],
            tool: None,
        }
    }

    #[test]
    fn height_cache_hits_for_same_block_profile_width_and_flags() {
        let cache = RenderHeightCache::default();
        let block = block("hello", false);
        let flags = RenderHeightFlags::default();

        let first = cache.height_for_block(
            &block,
            ThemeName::Dark,
            80,
            RendererProfile::Medium,
            flags,
            || 3,
        );
        let second = cache.height_for_block(
            &block,
            ThemeName::Dark,
            80,
            RendererProfile::Medium,
            flags,
            || 99,
        );

        assert_eq!(first, 3);
        assert_eq!(second, 3);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn height_cache_invalidates_on_width_content_expand_and_theme() {
        let cache = RenderHeightCache::default();
        let flags = RenderHeightFlags::default();
        let original = block("hello", false);

        let _ = cache.height_for_block(
            &original,
            ThemeName::Dark,
            80,
            RendererProfile::Medium,
            flags,
            || 1,
        );
        let _ = cache.height_for_block(
            &original,
            ThemeName::Dark,
            60,
            RendererProfile::Small,
            flags,
            || 2,
        );
        let _ = cache.height_for_block(
            &block("hello\nchanged", false),
            ThemeName::Dark,
            80,
            RendererProfile::Medium,
            flags,
            || 3,
        );
        let _ = cache.height_for_block(
            &block("hello", true),
            ThemeName::Dark,
            80,
            RendererProfile::Medium,
            flags,
            || 4,
        );
        let _ = cache.height_for_block(
            &original,
            ThemeName::Light,
            80,
            RendererProfile::Medium,
            flags,
            || 5,
        );

        assert_eq!(cache.stats().misses, 5);
        assert_eq!(cache.stats().hits, 0);
    }
}
