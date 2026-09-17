//! Errors raised while *registering* a tool.
//!
//! Errors raised *inside* a handler are not these — they travel back to the
//! agent as a `ToolResult` with `isError: true`, which is what the protocol
//! expects. See [`crate::ToolResult::error`].

/// Result alias for registration operations.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No `modelContext` on this page: an older browser, a non-browser target,
    /// or the server side of an SSR render.
    ///
    /// Callers should treat this as "the agent surface is not here" rather than
    /// as a failure — see [`crate::is_supported`].
    #[error("WebMCP is not available in this environment")]
    Unsupported,

    /// The user agent rejected the descriptor (bad name, malformed schema, or a
    /// name already registered on this page).
    #[error("registerTool() rejected the tool: {0}")]
    Rejected(String),

    /// A required descriptor field is missing or blank.
    #[error("invalid tool descriptor: {0}")]
    Descriptor(String),

    /// `inputSchema` did not serialize to a JSON object.
    #[error("invalid input schema: {0}")]
    Schema(String),
}
