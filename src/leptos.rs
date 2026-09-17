//! Leptos integration: tie a tool's lifetime to a component's.

use leptos::prelude::{Owner, StoredValue};

use crate::error::{Error, Result};
use crate::tool::ToolHandle;

/// Register a tool for as long as the calling component is mounted.
///
/// The handle is parked in the current reactive owner's arena, so cleaning that
/// owner up — which is what unmounting a component does — drops it and
/// unregisters the tool. That is the whole point: a tool that acts on the
/// screen the user is looking at should not survive them navigating away.
///
/// ```no_run
/// # use webmcp::{Tool, ToolResult};
/// # fn project_page() {
/// webmcp::leptos::use_tool(|| {
///     Tool::new("refresh_feedback")
///         .description("Reload the feedback list")
///         .register(|_| async { Ok::<_, std::convert::Infallible>(ToolResult::text("done")) })
/// });
/// # }
/// ```
///
/// Call it from a component body, which runs once. Calling it from an effect or
/// a loop re-registers the same name, which the browser rejects.
///
/// [`Error::Unsupported`] is ignored silently: it is the expected outcome
/// during SSR and on browsers without WebMCP, and warning about it once per
/// render on the server would be pure noise. Real failures are logged.
pub fn use_tool<F>(init: F)
where
    F: FnOnce() -> Result<ToolHandle>,
{
    match init() {
        Ok(handle) => {
            // Arena values are only reclaimed by the owner that registered
            // them. With no owner in scope the handle would live until the page
            // unloads, which is the ghost tool this function exists to prevent.
            if Owner::current().is_none() {
                crate::warn(&format!(
                    "webmcp: `{}` was registered outside a reactive owner \
                     and will stay registered until the page unloads",
                    handle.name()
                ));
            }
            StoredValue::new_local(handle);
        }
        Err(Error::Unsupported) => {}
        Err(e) => crate::warn(&format!("webmcp: {e}")),
    }
}
