//! Browser-side [WebMCP](https://github.com/webmachinelearning/webmcp) for Rust.
//!
//! WebMCP lets a *page* hand tools to whatever agent is driving the browser.
//! Unlike a hosted MCP server, the tools run inside the session the user is
//! already logged into: no API key, no second copy of your authorization rules,
//! and the agent sees exactly what the user can see.
//!
//! # What this crate adds over calling the API directly
//!
//! `js_sys::eval` can register a tool in five lines. What it cannot do is keep
//! the JSON Schema the agent plans against in sync with the struct the handler
//! destructures, or guarantee the tool disappears when the UI behind it does.
//! Those are the two failure modes this crate exists to remove:
//!
//! * [`Tool::register_typed`] derives `inputSchema` from the argument type, so
//!   the schema and the deserialization target are one declaration.
//! * [`ToolHandle`] unregisters on drop, which under the current spec means
//!   aborting the `AbortSignal` handed to `registerTool()`.
//!
//! # Targets
//!
//! Off wasm — SSR, native, tests — every registration returns
//! [`Error::Unsupported`] instead of failing to compile, so the same component
//! code builds for both halves of a fullstack app.
//!
//! # Spec drift
//!
//! The browser API is not stable yet. This crate targets the surface as of the
//! April 2026 draft: `registerTool(tool, { signal })` on `document.modelContext`,
//! falling back to the deprecated `navigator.modelContext` alias. The removed
//! `provideContext()` / `clearContext()` / `unregisterTool()` methods are not
//! implemented. See `wasm::model_context` for how the lookup degrades.
//!
//! # Example
//!
//! ```no_run
//! # #[cfg(feature = "schema")] {
//! use webmcp::{Tool, ToolResult};
//!
//! #[derive(serde::Deserialize, schemars::JsonSchema)]
//! struct AddTodo {
//!     /// The text content of the todo item.
//!     text: String,
//! }
//!
//! let handle = Tool::new("add_todo")
//!     .description("Add an item to the user's todo list")
//!     .register_typed(|args: AddTodo| async move {
//!         Ok::<_, std::convert::Infallible>(ToolResult::text(format!("Added {}", args.text)))
//!     })?;
//! // `handle` keeps the tool alive; dropping it unregisters.
//! # }
//! # Ok::<(), webmcp::Error>(())
//! ```

mod error;
mod result;
mod tool;

#[cfg(feature = "schema")]
mod schema;

#[cfg(target_family = "wasm")]
pub mod wasm;
#[cfg(target_family = "wasm")]
use wasm as imp;

#[cfg(not(target_family = "wasm"))]
mod native;
#[cfg(not(target_family = "wasm"))]
use native as imp;

// Public so both can coexist under `--all-features`; the root `use_tool` alias
// below covers the ordinary case of a crate using exactly one framework.
#[cfg(feature = "dioxus")]
pub mod dioxus;
#[cfg(feature = "leptos")]
pub mod leptos;

pub use error::{Error, Result};
pub use result::{Content, ToolResult};
pub use tool::{Tool, ToolHandle};

#[cfg(feature = "schema")]
pub use schema::schema_for;

#[cfg(feature = "dioxus")]
pub use dioxus::use_tool;
#[cfg(all(feature = "leptos", not(feature = "dioxus")))]
pub use leptos::use_tool;

/// Whether the current page exposes a model context to register tools on.
///
/// Useful for showing an "agent-ready" affordance, not for guarding
/// registration — registering on an unsupported page is already a harmless
/// [`Error::Unsupported`].
pub fn is_supported() -> bool {
    imp::is_supported()
}

/// Warn on the browser console; a no-op elsewhere.
///
/// The crate deliberately has no logging dependency: a UI crate should not
/// force `tracing` or `log` on its dependents for three diagnostics.
///
/// Only wasm and the framework features call it; the plain host build would
/// otherwise flag it as dead.
#[cfg_attr(
    not(any(target_family = "wasm", feature = "dioxus", feature = "leptos")),
    allow(dead_code)
)]
pub(crate) fn warn(message: &str) {
    #[cfg(target_family = "wasm")]
    web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(message));
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = message;
    }
}
