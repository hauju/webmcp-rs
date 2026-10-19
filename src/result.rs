//! What a tool hands back to the agent.

use serde::Serialize;

/// One block of a tool's reply. Only text blocks are modelled: the browser API
/// carries the same `content` array as MCP proper, but a page has no business
/// streaming binary blobs at an agent, and every richer block type would need a
/// matching browser-side sanitiser.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Content {
    Text { text: String },
}

/// A tool's reply.
///
/// `is_error` is the protocol's *soft* failure channel: the call completed, the
/// agent gets to read why it did not work and try something else. Registration
/// failures are the hard channel and use [`crate::Error`] instead.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub content: Vec<Content>,
    #[serde(skip_serializing_if = "is_false")]
    pub is_error: bool,
}

pub(crate) fn is_false(b: &bool) -> bool {
    !*b
}

impl ToolResult {
    /// A successful reply carrying a single block of prose.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text { text: text.into() }],
            is_error: false,
        }
    }

    /// A successful reply carrying `value` as pretty-printed JSON.
    ///
    /// Agents parse JSON out of text blocks reliably; pretty-printing costs a
    /// few tokens and buys a much better chance the model reads nested data
    /// correctly. Falls back to an error result if `value` cannot serialize.
    pub fn json<T: Serialize>(value: &T) -> Self {
        match serde_json::to_string_pretty(value) {
            Ok(json) => Self::text(json),
            Err(e) => Self::error(format!("failed to serialize result: {e}")),
        }
    }

    /// A failed reply. The agent sees the message and can correct its call.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text {
                text: message.into(),
            }],
            is_error: true,
        }
    }
}
