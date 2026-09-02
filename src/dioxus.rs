//! Dioxus integration: tie a tool's lifetime to a component's.

use std::rc::Rc;

use dioxus::prelude::use_hook;

use crate::error::{Error, Result};
use crate::tool::ToolHandle;

/// Register a tool for as long as the calling component is mounted.
///
/// The handle is parked in the component's hook state, so unmounting drops it
/// and unregisters the tool. That is the whole point: a tool that acts on the
/// screen the user is looking at should not survive them navigating away.
///
/// ```no_run
/// # use dioxus::prelude::*;
/// # use webmcp::{Tool, ToolResult};
/// # fn ProjectPage(project_id: String) -> Element {
/// webmcp::use_tool(|| {
///     Tool::new("refresh_feedback")
///         .description("Reload the feedback list")
///         .register(|_| async { Ok::<_, std::convert::Infallible>(ToolResult::text("done")) })
/// });
/// # todo!()
/// # }
/// ```
///
/// Registration runs once, on mount. Hooks are positional, so call this
/// unconditionally — the same rule as every other Dioxus hook.
///
/// [`Error::Unsupported`] is ignored silently: it is the expected outcome
/// during SSR and on browsers without WebMCP, and warning about it once per
/// render on the server would be pure noise. Real failures are logged.
pub fn use_tool<F>(init: F)
where
    F: FnOnce() -> Result<ToolHandle>,
{
    use_hook(|| match init() {
        Ok(handle) => Some(Rc::new(handle)),
        Err(Error::Unsupported) => None,
        Err(e) => {
            crate::warn(&format!("webmcp: {e}"));
            None
        }
    });
}
