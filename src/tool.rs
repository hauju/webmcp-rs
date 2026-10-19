//! Tool descriptors and the registration handle.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::result::ToolResult;

/// A handler with its argument- and error-types erased.
///
/// Not `Send`: WebMCP only exists on the main thread of a document, and
/// requiring `Send` here would rule out the `Rc`-based state every wasm UI
/// framework hands you.
pub(crate) type ErasedHandler = Rc<dyn Fn(Value) -> Pin<Box<dyn Future<Output = ToolResult>>>>;

/// Builder for a tool the page offers to agents.
///
/// ```no_run
/// # use webmcp::{Tool, ToolResult};
/// let handle = Tool::new("add_todo")
///     .description("Add an item to the user's todo list")
///     .input_schema(serde_json::json!({
///         "type": "object",
///         "properties": { "text": { "type": "string" } },
///         "required": ["text"]
///     }))
///     .register(|args| async move {
///         let text = args["text"].as_str().unwrap_or_default().to_string();
///         Ok::<_, std::convert::Infallible>(ToolResult::text(format!("Added {text}")))
///     })?;
/// # Ok::<(), webmcp::Error>(())
/// ```
pub struct Tool {
    pub(crate) name: String,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) input_schema: Value,
    pub(crate) annotations: Annotations,
    pub(crate) exposed_to: Vec<String>,
}

/// The descriptor's behaviour hints. Every hint defaults to `false` and the
/// guidance is to send only the ones that are `true`, so `false` fields are
/// skipped and an all-`false` set is left off the descriptor entirely.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Annotations {
    #[serde(skip_serializing_if = "crate::result::is_false")]
    pub read_only_hint: bool,
    #[serde(skip_serializing_if = "crate::result::is_false")]
    pub untrusted_content_hint: bool,
    #[serde(skip_serializing_if = "crate::result::is_false")]
    pub consequential_hint: bool,
    #[serde(skip_serializing_if = "crate::result::is_false")]
    pub debugging: bool,
}

impl Tool {
    /// Start a tool named `name`. Names are page-global: registering a name
    /// that is already taken is an error, not an overwrite. A name is 1–128
    /// ASCII letters, digits, `_`, `-` or `.`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            description: None,
            // A tool with no declared parameters still needs an object schema —
            // agents reject a descriptor whose inputSchema is absent or scalar.
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            annotations: Annotations::default(),
            exposed_to: Vec::new(),
        }
    }

    /// A human-readable label for the tool, for UIs that list tools.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// What the tool does, in the terms the *model* needs to pick it. This is
    /// the single highest-leverage field on the descriptor: say what it does
    /// and when to use it ("Adds an item to the cart. Use when …").
    /// A non-blank description is required before registration.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the JSON Schema for the handler's arguments.
    pub fn input_schema(mut self, schema: Value) -> Self {
        self.input_schema = schema;
        self
    }

    /// Derive the argument schema from `T` instead of writing it by hand.
    #[cfg(feature = "schema")]
    pub fn args<T: schemars::JsonSchema>(mut self) -> Self {
        self.input_schema = crate::schema::schema_for::<T>();
        self
    }

    /// Mark the tool as a pure query: it changes neither application state nor
    /// the view. A tool that navigates or switches tabs is not read-only.
    pub fn read_only(mut self) -> Self {
        self.annotations.read_only_hint = true;
        self
    }

    /// Mark the tool's output as possibly containing text written by users or
    /// third parties (comments, reviews, tickets) — even from your own
    /// database — so the agent treats it as data, not instructions.
    pub fn untrusted_content(mut self) -> Self {
        self.annotations.untrusted_content_hint = true;
        self
    }

    /// Mark the tool as significant, real-world or irreversible (a payment, a
    /// booking, a deletion), so the agent can confirm with the user first.
    pub fn consequential(mut self) -> Self {
        self.annotations.consequential_hint = true;
        self
    }

    /// Mark the tool as developer tooling rather than an end-user action.
    pub fn debugging(mut self) -> Self {
        self.annotations.debugging = true;
        self
    }

    /// Expose this tool to the listed origins as well as this one. Empty (the
    /// default) keeps it same-origin.
    pub fn exposed_to(mut self, origins: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.exposed_to = origins.into_iter().map(Into::into).collect();
        self
    }

    /// Register the tool, returning a handle that unregisters it when dropped.
    ///
    /// `handler` may fail with any `Display` error — `?` works inside the async
    /// block, and the error text reaches the agent as an error result.
    ///
    /// The browser's `registerTool()` is asynchronous, so a rejection there
    /// (a duplicate name, say) surfaces on the console rather than in this
    /// return value; the `Err` cases here are the synchronous ones. Use
    /// [`register_async`](Self::register_async) when you need to await the
    /// verdict.
    pub fn register<F, Fut, E>(self, handler: F) -> Result<ToolHandle>
    where
        F: Fn(Value) -> Fut + 'static,
        Fut: Future<Output = std::result::Result<ToolResult, E>> + 'static,
        E: std::fmt::Display + 'static,
    {
        crate::imp::register(self, erase(handler))
    }

    /// Like [`register`](Self::register), but awaits the user agent's verdict so
    /// a rejected descriptor is a real `Err`.
    pub async fn register_async<F, Fut, E>(self, handler: F) -> Result<ToolHandle>
    where
        F: Fn(Value) -> Fut + 'static,
        Fut: Future<Output = std::result::Result<ToolResult, E>> + 'static,
        E: std::fmt::Display + 'static,
    {
        crate::imp::register_async(self, erase(handler)).await
    }

    /// Register with the argument schema derived from `T` and the arguments
    /// deserialized into it.
    ///
    /// This is the reason to reach for Rust here at all: the schema the agent
    /// plans against and the struct the handler destructures are generated from
    /// one declaration, so they cannot drift apart.
    #[cfg(feature = "schema")]
    pub fn register_typed<T, F, Fut, E>(self, handler: F) -> Result<ToolHandle>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + 'static,
        F: Fn(T) -> Fut + 'static,
        Fut: Future<Output = std::result::Result<ToolResult, E>> + 'static,
        E: std::fmt::Display + 'static,
    {
        crate::imp::register(self.args::<T>(), erase_typed(handler))
    }

    /// Like [`register_typed`](Self::register_typed), but awaits the browser's
    /// verdict. Schema derivation and argument decoding are identical; browser
    /// rejections (such as a duplicate name) are returned as [`Error::Rejected`].
    #[cfg(feature = "schema")]
    pub async fn register_typed_async<T, F, Fut, E>(self, handler: F) -> Result<ToolHandle>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + 'static,
        F: Fn(T) -> Fut + 'static,
        Fut: Future<Output = std::result::Result<ToolResult, E>> + 'static,
        E: std::fmt::Display + 'static,
    {
        crate::imp::register_async(self.args::<T>(), erase_typed(handler)).await
    }
}

#[cfg(feature = "schema")]
fn erase_typed<T, F, Fut, E>(handler: F) -> ErasedHandler
where
    T: serde::de::DeserializeOwned + 'static,
    F: Fn(T) -> Fut + 'static,
    Fut: Future<Output = std::result::Result<ToolResult, E>> + 'static,
    E: std::fmt::Display + 'static,
{
    erase(move |args| {
        let call = serde_json::from_value::<T>(args).map(&handler);
        async move {
            match call {
                Ok(fut) => fut.await.map_err(|e| e.to_string()),
                Err(e) => Ok(ToolResult::error(format!("invalid arguments: {e}"))),
            }
        }
    })
}

fn erase<F, Fut, E>(handler: F) -> ErasedHandler
where
    F: Fn(Value) -> Fut + 'static,
    Fut: Future<Output = std::result::Result<ToolResult, E>> + 'static,
    E: std::fmt::Display + 'static,
{
    Rc::new(move |args| {
        let fut = handler(args);
        Box::pin(async move {
            match fut.await {
                Ok(result) => result,
                Err(e) => ToolResult::error(e.to_string()),
            }
        })
    })
}

/// Keeps a tool registered. Dropping it unregisters the tool.
///
/// The spec dropped `unregisterTool()` in favour of aborting an `AbortSignal`
/// handed to `registerTool()`, which maps exactly onto Rust's ownership: hold
/// the handle for as long as the UI that backs the tool exists, and the tool
/// disappears with it. Ghost tools — registered against a component that has
/// since unmounted — are the failure mode that motivated the API change, and
/// this type is how you avoid them.
#[must_use = "dropping the handle immediately unregisters the tool"]
pub struct ToolHandle {
    pub(crate) name: String,
    /// Held only for its `Drop`, which aborts the registration's signal.
    #[cfg(target_family = "wasm")]
    pub(crate) _registration: crate::wasm::Registration,
}

impl ToolHandle {
    /// The registered tool's name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Debug for ToolHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolHandle")
            .field("name", &self.name)
            .finish()
    }
}

/// Validate the descriptor before it reaches the browser.
pub(crate) fn validate_descriptor(tool: &Tool) -> Result<()> {
    if tool.name.trim().is_empty() {
        return Err(Error::Descriptor("tool name must not be empty".into()));
    }
    if tool.name.len() > 128
        || !tool
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(Error::Descriptor(format!(
            "tool name `{}` must be 1-128 ASCII letters, digits, `_`, `-` or `.`",
            tool.name
        )));
    }
    if tool
        .description
        .as_deref()
        .is_none_or(|text| text.trim().is_empty())
    {
        return Err(Error::Descriptor(
            "tool description must not be empty".into(),
        ));
    }
    if !tool.input_schema.is_object() {
        return Err(Error::Schema(format!(
            "inputSchema for `{}` must be a JSON object, got {}",
            tool.name, tool.input_schema
        )));
    }
    Ok(())
}
