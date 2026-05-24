//! Image-processing helpers — mirror of `tools/FileReadTool/imageProcessor.ts`.
//!
//! The TS version dynamically loads `sharp` via `import('sharp')`. In the
//! Rust port we don't ship a built-in image processor; these helpers expose
//! pluggable closures so the runtime can swap in an `image`-crate backend.

use once_cell::sync::Lazy;
use std::sync::Mutex;

/// `imageProcessor.ts` `SharpInstance` — opaque processor handle.
#[derive(Debug, Clone)]
pub struct SharpInstance {
    pub kind: String,
}

/// `imageProcessor.ts` `SharpFunction` — boxed processor closure (resize +
/// re-encode). Inputs: raw bytes, max width, max height, quality.
pub type SharpFunction =
    Box<dyn Fn(&[u8], u32, u32, u8) -> Result<Vec<u8>, String> + Send + Sync + 'static>;

static IMAGE_PROCESSOR: Lazy<Mutex<Option<SharpInstance>>> = Lazy::new(|| Mutex::new(None));
static IMAGE_CREATOR: Lazy<Mutex<Option<SharpFunction>>> = Lazy::new(|| Mutex::new(None));

/// `imageProcessor.ts` `getImageProcessor` — returns the configured processor
/// instance, if any.
pub fn get_image_processor() -> Option<SharpInstance> {
    IMAGE_PROCESSOR.lock().unwrap().clone()
}

/// Install an image processor instance.
pub fn set_image_processor(instance: SharpInstance) {
    *IMAGE_PROCESSOR.lock().unwrap() = Some(instance);
}

/// `imageProcessor.ts` `getImageCreator` — returns whether a processor
/// closure is installed (closure itself is not cloned).
pub fn get_image_creator() -> bool {
    IMAGE_CREATOR.lock().unwrap().is_some()
}

/// Install a processor closure.
pub fn set_image_creator(f: SharpFunction) {
    *IMAGE_CREATOR.lock().unwrap() = Some(f);
}

/// Invoke the installed processor closure (no-op if unset).
pub fn process_image(bytes: &[u8], w: u32, h: u32, quality: u8) -> Result<Vec<u8>, String> {
    let guard = IMAGE_CREATOR.lock().unwrap();
    match guard.as_ref() {
        Some(f) => f(bytes, w, h, quality),
        None => Err("no image processor installed".to_string()),
    }
}
