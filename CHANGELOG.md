# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Initial release. `Tool` builder with `register`, `register_async` and
  `register_typed` (schema derived from the argument type via `schemars`);
  `ToolHandle` that unregisters on drop by aborting the registration's
  `AbortSignal`; `ToolResult` in the protocol's `content` / `isError` shape;
  `use_tool` for Dioxus behind the `dioxus` feature. Off wasm, registration
  reports `Error::Unsupported` instead of failing to compile.
- Tracks the April 2026 WebMCP draft: `document.modelContext.registerTool(tool,
  { signal })`, with `navigator.modelContext` as a fallback.
