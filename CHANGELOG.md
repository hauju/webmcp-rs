# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `register_typed_async`, combining derived schemas with awaited registration.
- `leptos` feature with `leptos::use_tool`, tying a registration to the calling
  component's reactive owner. It warns when no owner is in scope, because an
  arena value with no owner to reclaim it outlives the UI it came from. The
  `dioxus` and `leptos` modules are now public so both can be enabled at once;
  with exactly one enabled, `use_tool` is still re-exported at the crate root.
- `Tool::title` and the opt-in behaviour hints `read_only`, `untrusted_content`,
  `consequential` and `debugging`, sent as the descriptor's `annotations`.
- Honour the execution `AbortSignal`: a cancelled call drops the handler's
  future and resolves with a `cancelled` error result.
- Browser integration tests for registration, argument decoding, tool lifetimes,
  demo mutation acknowledgements, and event listener cleanup, run in CI.

### Fixed

- Validate tool names against the spec (1–128 ASCII letters, digits, `_`, `-`,
  `.`) with `Error::Descriptor`.
- Resolve instead of rejecting when a tool result fails to encode; a rejection
  reaches the agent as a bare `UnknownError`.

- Reject missing or blank tool descriptions locally with `Error::Descriptor`;
  blank names now use the same error instead of `Error::Schema`.
- Return error results for invalid JS arguments without invoking handlers.
  Only omitted arguments default to an empty object; explicit null is rejected.
- Await applied demo mutations and refresh the snapshot before replying. Added
  todos return their IDs; missing IDs are checked against the current state.
- Remove the console's `toolchange` listener before dropping its callback.

### Initial implementation

- Initial release. `Tool` builder with `register`, `register_async` and
  `register_typed` (schema derived from the argument type via `schemars`);
  `ToolHandle` that unregisters on drop by aborting the registration's
  `AbortSignal`; `ToolResult` in the protocol's `content` / `isError` shape;
  `use_tool` for Dioxus behind the `dioxus` feature. Off wasm, registration
  reports `Error::Unsupported` instead of failing to compile.
- Tracks the April 2026 WebMCP draft: `document.modelContext.registerTool(tool,
  { signal })`, with `navigator.modelContext` as a fallback.
