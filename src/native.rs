//! Everything off the web: server-side rendering, native builds, tests.
//!
//! Registration is a no-op that reports [`Error::Unsupported`] rather than a
//! compile error, so the same UI code can be built for both a wasm client and
//! an SSR server without `cfg` noise at every call site.

use crate::error::{Error, Result};
use crate::tool::{ErasedHandler, Tool, ToolHandle, validate_schema};

pub fn is_supported() -> bool {
    false
}

pub fn register(tool: Tool, _handler: ErasedHandler) -> Result<ToolHandle> {
    // Validate anyway, so a malformed descriptor is caught by a plain
    // `cargo test` instead of only in a browser.
    validate_schema(&tool)?;
    Err(Error::Unsupported)
}

pub async fn register_async(tool: Tool, handler: ErasedHandler) -> Result<ToolHandle> {
    register(tool, handler)
}
