# webmcp-rs

[![crates.io](https://img.shields.io/crates/v/webmcp-rs.svg)](https://crates.io/crates/webmcp-rs)
[![docs.rs](https://img.shields.io/docsrs/webmcp-rs)](https://docs.rs/webmcp-rs)
[![CI](https://github.com/hauju/webmcp-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/hauju/webmcp-rs/actions/workflows/ci.yml)

Browser-side [WebMCP](https://github.com/webmachinelearning/webmcp) for Rust and
WebAssembly: let the page hand tools to whatever agent is driving the browser.

**Live demo: <https://webmcp.oxidt.com>** — a todo list driven by five
registered tools, with a console for calling them by hand.

WebMCP is the W3C proposal (Google + Microsoft, Web Machine Learning CG) that
lets a web page register tools on `document.modelContext`. An in-browser agent
discovers them and calls them instead of scraping the DOM. The tools run inside
the session the user is already logged into: no API key, no second copy of your
authorization rules, and the agent sees exactly what the user can see.

```toml
[dependencies]
webmcp-rs = "0.1"
```

The package is `webmcp-rs` (the bare `webmcp` name on crates.io is a
placeholder); the library is `webmcp`, so code reads naturally:

```rust
use webmcp::{Tool, ToolResult};

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct AddTodo {
    /// The text content of the todo item.
    text: String,
}

let handle = Tool::new("add_todo")
    .description("Add an item to the user's todo list")
    .register_typed(|args: AddTodo| async move {
        Ok::<_, std::convert::Infallible>(ToolResult::text(format!("Added {}", args.text)))
    })?;
// `handle` keeps the tool alive; dropping it unregisters.
```

## Why not just `js_sys::eval`

Five lines of `eval` will register a tool. The two things it will not do are the
two things that break in practice:

- **Schema drift.** The JSON Schema the agent plans against and the struct the
  handler destructures are written twice and diverge on the first refactor.
  `register_typed` derives the schema from the argument type (doc comments
  become field descriptions), so they are one declaration.
- **Ghost tools.** A tool registered for a component that has since unmounted
  still answers calls, acting on state the user is no longer looking at. This is
  the failure mode that got `provideContext()` removed from the spec.
  `ToolHandle` unregisters on drop; with the `dioxus` feature, `use_tool` parks
  it in a component's hook state so unmounting is enough.

## Dioxus

```rust
use dioxus::prelude::*;
use webmcp::{Tool, ToolResult};

#[component]
fn ProjectPage(project_id: String) -> Element {
    webmcp::use_tool(move || {
        Tool::new("refresh_feedback")
            .description("Reload the feedback list")
            .register(|_| async { Ok::<_, std::convert::Infallible>(ToolResult::text("done")) })
    });
    rsx! { /* ... */ }
}
```

Enable it with `features = ["dioxus"]`. Handlers run on a browser callback,
outside the framework's runtime: reach for `Rc<RefCell<_>>` or a coroutine
channel for state, not a `Signal`.

## Handlers

- `register(|args: serde_json::Value| ...)` — hand-written `inputSchema`.
- `register_typed(|args: T| ...)` — schema derived from `T: JsonSchema`
  (`schema` feature, on by default). Bad arguments come back to the agent as an
  error result naming the field, not as a panic.
- `register_async(...)` — awaits the browser's verdict, so a rejected
  descriptor (a duplicate name, say) is a real `Err` instead of a console
  warning.
- `register_typed_async(...)` — combines derived schemas and typed arguments
  with the awaited registration verdict.

Every tool needs a non-blank `.description(...)`. Missing descriptions and blank
names return `Error::Descriptor` before reaching the browser, including on native
targets. Invalid JS arguments return an error result without invoking the handler;
only an omitted (`undefined`) argument defaults to an empty object.

A handler returns `Result<ToolResult, E>` for any `E: Display`; an `Err` reaches
the agent as `isError: true` with the message, which is the protocol's soft
failure channel. `ToolResult::json(&value)` pretty-prints structured data into a
text block, which models read more reliably than compact JSON.

## Targets

Off wasm — SSR, native, tests — registration returns `Error::Unsupported`
instead of failing to compile, so the same component code builds for both halves
of a fullstack app. Descriptor validation still runs there, so `cargo test`
catches a malformed schema without a browser.

## Browser requirements

- Chrome 149+ in the [origin trial](https://developer.chrome.com/blog/ai-webmcp-origin-trial),
  or locally with `chrome://flags/#enable-webmcp-testing` enabled. Edge ships it
  behind a flag.
- The page must be origin-isolated, which is the default. Do **not** send
  `Origin-Agent-Cluster: ?0` (it re-enables `document.domain` and disables
  WebMCP).
- The `tools` Permissions Policy defaults to `self`: top-level and same-origin
  frames may register, cross-origin iframes need `allow="tools"`.

`webmcp::is_supported()` reports whether the current page has a model context,
for an "agent-ready" affordance. Registering on an unsupported page is already a
harmless `Error::Unsupported`, so there is no need to guard on it.

## Spec drift

The browser API is not stable. This crate tracks the April 2026 draft:

| | Status |
|---|---|
| `document.modelContext.registerTool(tool, { signal })` | what this crate calls |
| `navigator.modelContext` | deprecated alias, used as a fallback |
| `provideContext()` / `clearContext()` | removed March 2026, not implemented |
| `unregisterTool()` | removed April 2026, replaced by aborting the signal |
| Declarative API (`toolname` on `<form>`) | out of scope; plain HTML already covers it |

The interface is reached reflectively rather than through `extern` bindings, so
a browser with an older or different surface degrades to `Error::Unsupported`
rather than throwing.

## Features

- `schema` (default) — `register_typed` / `Tool::args::<T>()` via `schemars`.
- `dioxus` — `use_tool`, tying a registration to a component's lifetime.

## Site

`site/` is the Dioxus web app behind the live demo: one client-rendered page,
excluded from the published crate, deployed to GitHub Pages by
`.github/workflows/site.yml` on every push to `main`.

```sh
just site-serve   # http://localhost:8080/
just site-build   # exactly what CI publishes
```

`site/src/demo.rs` is the demo in full — five tools registered with `use_tool`,
and the read-snapshot / coroutine-channel pattern that keeps handlers off the
Dioxus runtime. Mutation calls await a reply after the state and read snapshot
are updated; `add_todo` returns the created item, including its ID. The page
shows that file verbatim.

The demo works in any browser because the page loads Google's
[WebMCP polyfill](https://github.com/GoogleChromeLabs/webmcp-tools) (Apache-2.0)
before the wasm starts; it is vendored at `site/assets/webmcp-polyfill.js` with
its copyright header intact, and stands down when the browser has the native
API.

## Testing

`just test` runs host tests and checks the docs. `just test-browser` runs the
crate's WASM tests with and without default features, plus the demo tests, in
headless Chrome using `wasm-pack`. The browser suite installs a controlled model
context to exercise registration, execution, errors, and cleanup without an
experimental browser flag. It also mounts Dioxus components to check tool
lifetimes and reads immediately following mutations. This validates the Rust/JS
integration; it does not certify a particular native WebMCP implementation.

## License

MIT
