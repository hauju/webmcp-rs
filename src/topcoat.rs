//! Server-rendered tools for [Topcoat](https://github.com/tokio-rs/topcoat).
//!
//! Topcoat apps ship no WebAssembly, so tools cannot run in the browser.
//! Instead the page renders [`webmcp_tools`], a small inline script that
//! registers each [`Tool`] on `document.modelContext` and forwards every call
//! as a `POST` of a [`ToolCall`] to an endpoint you route. The handler runs on
//! the server, inside the request of the user who is signed in, so it is
//! authorized exactly like the rest of your routes.
//!
//! ```ignore
//! use topcoat::{Result, context::Cx, router::{content::Json, page, route}, view::{View, view}};
//! use webmcp::{Tool, ToolResult, topcoat::{ToolCall, webmcp_tools}};
//!
//! #[derive(serde::Deserialize, schemars::JsonSchema)]
//! struct AddTodo {
//!     /// The text content of the todo item.
//!     text: String,
//! }
//!
//! fn tools() -> Vec<Tool> {
//!     vec![Tool::new("add_todo").description("Add an item to the user's todo list").args::<AddTodo>()]
//! }
//!
//! #[page("/")]
//! async fn home() -> Result<impl View> {
//!     let tools = tools();
//!     Ok(view! { webmcp_tools(tools: &tools, endpoint: "/webmcp") })
//! }
//!
//! #[route(POST "/webmcp")]
//! async fn call_tool(cx: &Cx, Json(call): Json<ToolCall>) -> Result<Json<ToolResult>> {
//!     Ok(Json(match call.name.as_str() {
//!         "add_todo" => call.run(|args: AddTodo| add_todo(cx, args)).await,
//!         _ => call.unknown(),
//!     }))
//! }
//! ```
//!
//! The tools live as long as the document. A re-run of the script (an HTMX
//! swap, say) replaces the previous set rather than colliding with it.
//!
//! Requires Rust 1.99, Topcoat's minimum. Host-only: the feature compiles to
//! nothing on wasm.

use std::future::Future;

use ::topcoat::{
    Result,
    view::{Unescaped, View, component, view},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::result::ToolResult;
use crate::tool::{Tool, validate_descriptor};

const BRIDGE: &str = include_str!("topcoat.js");

/// Register `tools` on the page, forwarding calls to `endpoint`.
///
/// `endpoint` is a same-origin URL whose `POST` handler takes a [`ToolCall`]
/// and answers with a [`ToolResult`]. Rendering fails if a descriptor is
/// invalid, the same checks [`Tool::register`] runs in the browser.
#[component]
pub async fn webmcp_tools(tools: &[Tool], endpoint: &str) -> Result<impl View> {
    let descriptors = tools
        .iter()
        .map(descriptor)
        .collect::<crate::Result<Vec<_>>>()?;
    // The source's comments are for maintainers, not for every page load.
    let bridge: String = BRIDGE
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let script = format!(
        "{}({}, {});",
        bridge.trim_end(),
        script_json(&json!(endpoint)),
        script_json(&Value::Array(descriptors)),
    );
    // Both arguments are JSON with every `<` escaped, so neither can close the
    // `<script>` element or open a comment inside it.
    Ok(view! { <script>(Unescaped::new_unchecked(script))</script> })
}

/// The descriptor `registerTool()` receives, minus `execute`, plus the
/// registration's `exposedTo`, which the script splits back out.
fn descriptor(tool: &Tool) -> crate::Result<Value> {
    validate_descriptor(tool)?;
    let mut out = json!({
        "name": tool.name,
        "description": tool.description,
        "inputSchema": tool.input_schema,
    });
    if let Some(title) = &tool.title {
        out["title"] = json!(title);
    }
    if tool.annotations != Default::default() {
        out["annotations"] = json!(tool.annotations);
    }
    if !tool.exposed_to.is_empty() {
        out["exposedTo"] = json!(tool.exposed_to);
    }
    Ok(out)
}

/// JSON safe to embed in a `<script>`. `<` only ever appears inside strings,
/// where `<` decodes back to the same character.
fn script_json(value: &Value) -> String {
    value.to_string().replace('<', "\\u003c")
}

/// One forwarded tool call: the body of the `POST` the script sends.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    /// The registered tool's name.
    pub name: String,
    /// The arguments the agent supplied, normally a JSON object.
    #[serde(default)]
    pub arguments: Value,
}

impl ToolCall {
    /// Decode the arguments into `T` and run `handler` on them.
    ///
    /// Undecodable arguments and handler errors both become error results the
    /// agent can read and correct, as in [`Tool::register_typed`].
    pub async fn run<T, F, Fut, E>(&self, handler: F) -> ToolResult
    where
        T: serde::de::DeserializeOwned,
        F: FnOnce(T) -> Fut,
        Fut: Future<Output = std::result::Result<ToolResult, E>>,
        E: std::fmt::Display,
    {
        // A call with no arguments sends `{}`, but an absent field decodes to
        // `null`; treat both as "no arguments".
        let empty = Value::Object(Default::default());
        let args = if self.arguments.is_null() {
            &empty
        } else {
            &self.arguments
        };
        match T::deserialize(args) {
            Ok(args) => handler(args)
                .await
                .unwrap_or_else(|e| ToolResult::error(e.to_string())),
            Err(e) => ToolResult::error(format!("invalid arguments: {e}")),
        }
    }

    /// The reply for a name the endpoint does not handle.
    pub fn unknown(&self) -> ToolResult {
        ToolResult::error(format!("unknown tool `{}`", self.name))
    }
}
