//! The webmcp-rs site: one page, client-rendered, no router.

mod console;
mod demo;

use dioxus::prelude::*;
use wasm_bindgen::JsValue;

use console::{AgentConsole, RegisteredTools};
use demo::TodoDemo;

// Both of these are referenced by name from `index.html`, not from Rust, so the
// hash suffix is off and `#[used]` keeps them in the bundle. The polyfill has to
// be a plain `<script>` in the head: it must install `document.modelContext`
// before the wasm renders and registers tools, which rules out injecting it from
// a component. The stylesheet rides along the same way to avoid a flash of
// unstyled page while a runtime-injected `<link>` loads.
#[used]
static POLYFILL: Asset = asset!(
    "/assets/webmcp-polyfill.js",
    AssetOptions::js()
        .with_minify(false)
        .with_hash_suffix(false)
);
// The generic builder, not `AssetOptions::css()`: as of manganis-core 0.7.10
// the css builder's `into_asset_options` hardcodes `add_hash: true`, so
// `with_hash_suffix(false)` is silently ignored there. Tailwind is told to
// `--minify` instead, which is the only thing the css variant would have added.
#[used]
static TAILWIND: Asset = asset!(
    "/assets/tailwind.css",
    AssetOptions::builder().with_hash_suffix(false)
);

/// The demo's source, shown verbatim further down the page. `demo.rs` is a
/// self-contained module so this stays honest: what you read is what runs.
const DEMO_SOURCE: &str = include_str!("demo.rs");

const REPO: &str = "https://github.com/hauju/webmcp-rs";
const CRATES_IO: &str = "https://crates.io/crates/webmcp-rs";
const DOCS_RS: &str = "https://docs.rs/webmcp-rs";
const SPEC: &str = "https://github.com/webmachinelearning/webmcp";
const CHROME_DOCS: &str = "https://developer.chrome.com/docs/ai/webmcp";
const ORIGIN_TRIAL: &str = "https://developer.chrome.com/blog/ai-webmcp-origin-trial";
const INSPECTOR: &str = "https://github.com/beaufortfrancois/model-context-tool-inspector";

fn main() {
    dioxus::launch(App);
}

/// Where the page's model context came from.
#[derive(Clone, Copy, PartialEq)]
enum Backing {
    Native,
    Polyfill,
    Missing,
}

impl Backing {
    /// `index.html` records `window.__webmcp_native` before the polyfill runs,
    /// which is the only moment the two are still distinguishable.
    fn detect() -> Self {
        if !webmcp::is_supported() {
            return Self::Missing;
        }
        let native = web_sys::window()
            .and_then(|window| {
                js_sys::Reflect::get(&window, &JsValue::from_str("__webmcp_native")).ok()
            })
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if native { Self::Native } else { Self::Polyfill }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Native => "Native WebMCP",
            Self::Polyfill => "Polyfill",
            Self::Missing => "Unavailable",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::Native => "This browser exposes document.modelContext itself.",
            Self::Polyfill => {
                "No native model context here, so the page installed Google's polyfill."
            }
            Self::Missing => {
                "Nothing to register on: every registration returns Error::Unsupported."
            }
        }
    }

    fn dot(self) -> &'static str {
        match self {
            Self::Native => "bg-primary",
            Self::Polyfill => "bg-secondary",
            Self::Missing => "bg-error",
        }
    }
}

#[component]
fn App() -> Element {
    let backing = use_hook(Backing::detect);

    rsx! {
        // No background here: the body carries the engineering grid.
        div { class: "min-h-screen text-base-content",
            TopBar {}
            main { class: "mx-auto w-full max-w-6xl px-5 pb-24 sm:px-8",
                Hero { backing }
                WhatItIs {}
                Demo { backing }
                RealAgents {}
                TheCode {}
            }
            SiteFooter {}
        }
    }
}

#[component]
fn TopBar() -> Element {
    rsx! {
        header { class: "sticky top-0 z-20 border-b border-base-300 bg-base-100/85 backdrop-blur",
            div { class: "mx-auto flex w-full max-w-6xl items-center justify-between px-5 py-3 sm:px-8",
                a { class: "font-mono text-sm font-medium tracking-tight", href: "#top",
                    span { class: "text-primary", "webmcp" }
                    span { class: "opacity-50", "-rs" }
                }
                nav { class: "flex items-center gap-5 font-mono text-xs",
                    a { class: "opacity-60 transition hover:opacity-100", href: "#demo", "demo" }
                    a { class: "opacity-60 transition hover:opacity-100", href: "#code", "code" }
                    a {
                        class: "opacity-60 transition hover:opacity-100",
                        href: DOCS_RS,
                        rel: "noopener",
                        "docs.rs"
                    }
                    a {
                        class: "opacity-60 transition hover:opacity-100",
                        href: REPO,
                        rel: "noopener",
                        "github"
                    }
                }
            }
        }
    }
}

#[component]
fn Hero(backing: Backing) -> Element {
    rsx! {
        section { id: "top", class: "grid gap-10 py-16 sm:py-24 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-end",
            div { class: "max-w-2xl",
                p { class: "rule-label", "Rust · WebAssembly · W3C WebMCP" }
                h1 { class: "mt-4 text-5xl font-bold tracking-tight sm:text-7xl",
                    span { class: "text-primary", "webmcp" }
                    "-rs"
                }
                p { class: "mt-5 text-xl leading-snug sm:text-2xl",
                    "Browser-side WebMCP for Rust and WebAssembly."
                }
                p { class: "mt-4 max-w-xl leading-relaxed opacity-70",
                    "Register tools on "
                    code { class: "text-primary", "document.modelContext" }
                    " so the agent driving the browser calls them instead of scraping your DOM. \
                     The JSON Schema is derived from your Rust argument type, and a registration \
                     unregisters when its handle drops."
                }

                div { class: "mt-8 flex flex-wrap items-center gap-3",
                    code { class: "rounded border border-base-300 bg-base-200 px-4 py-2.5 font-mono text-sm",
                        span { class: "select-none opacity-35", "$ " }
                        "cargo add webmcp-rs"
                    }
                    a {
                        class: "btn btn-sm btn-outline font-mono",
                        href: REPO,
                        rel: "noopener",
                        "GitHub"
                    }
                    a {
                        class: "btn btn-sm btn-ghost font-mono",
                        href: CRATES_IO,
                        rel: "noopener",
                        "crates.io"
                    }
                    a {
                        class: "btn btn-sm btn-ghost font-mono",
                        href: DOCS_RS,
                        rel: "noopener",
                        "docs.rs"
                    }
                }
            }

            aside { class: "w-full max-w-sm rounded-lg border border-base-300 bg-base-200/50 p-5",
                p { class: "rule-label", "this browser" }
                div { class: "mt-2 flex items-center gap-2.5",
                    span { class: "inline-block h-2 w-2 rounded-full wire-live {backing.dot()}" }
                    span { class: "font-mono text-sm", "{backing.label()}" }
                }
                p { class: "mt-3 text-xs leading-relaxed opacity-60", "{backing.note()}" }
            }
        }
    }
}

#[component]
fn WhatItIs() -> Element {
    rsx! {
        section { class: "border-t border-base-300 py-14",
            p { class: "rule-label", "01 / what webmcp is" }
            div { class: "mt-5 grid gap-6 lg:grid-cols-2",
                p { class: "text-lg leading-relaxed",
                    "WebMCP is a W3C proposal from the Web Machine Learning Community Group that lets a "
                    em { "page" }
                    " publish tools to whatever agent is driving the browser."
                }
                p { class: "leading-relaxed opacity-70",
                    "The tools run inside the session the user is already signed into, so there is no \
                     API key to mint and no second copy of your authorization rules to keep in step. \
                     Chrome has it behind "
                    code { "chrome://flags/#enable-webmcp-testing" }
                    " from 149 and in an origin trial. Read the "
                    a { class: "link link-primary", href: SPEC, rel: "noopener", "explainer" }
                    " or Chrome's "
                    a {
                        class: "link link-primary",
                        href: CHROME_DOCS,
                        rel: "noopener",
                        "developer docs"
                    }
                    "."
                }
            }
        }
    }
}

#[component]
fn Demo(backing: Backing) -> Element {
    rsx! {
        section { id: "demo", class: "scroll-mt-16 border-t border-base-300 py-14",
            p { class: "rule-label", "02 / demo" }
            h2 { class: "mt-4 text-3xl font-bold tracking-tight",
                "A todo list, and something pretending to be an agent"
            }
            p { class: "mt-3 max-w-2xl leading-relaxed opacity-70",
                "Both panels write to the same list. Add an item by hand on the left, or call "
                code { class: "text-primary", "add_todo" }
                " from the console on the right — the five tools are registered by "
                code { "webmcp::use_tool" }
                " and act on exactly the state you can see."
            }

            div { class: "mt-8 grid gap-px overflow-hidden rounded-lg border border-base-300 bg-base-300 lg:grid-cols-2",
                div { class: "flex min-w-0 flex-col gap-4 bg-base-200/40 p-5",
                    div { class: "flex items-center justify-between",
                        p { class: "rule-label", "the app" }
                        p { class: "font-mono text-[0.65rem] opacity-40", "src/demo.rs" }
                    }
                    TodoDemo {}
                }
                div { class: "flex min-w-0 flex-col gap-4 bg-base-200/40 p-5",
                    div { class: "flex items-center justify-between",
                        p { class: "rule-label", "the agent" }
                        p { class: "flex items-center gap-1.5 font-mono text-[0.65rem] opacity-40",
                            span { class: "inline-block h-1.5 w-1.5 rounded-full wire-live {backing.dot()}" }
                            "{backing.label()}"
                        }
                    }
                    AgentConsole {}
                }
            }

            div { class: "mt-10",
                p { class: "rule-label", "registered tools" }
                p { class: "mt-2 mb-4 max-w-2xl text-sm leading-relaxed opacity-60",
                    "Exactly what an agent sees. Names, descriptions and schemas all come from the \
                     Rust in "
                    code { "demo.rs" }
                    " — the descriptions from "
                    code { ".description(…)" }
                    ", the schemas from the argument structs and their doc comments."
                }
                RegisteredTools {}
            }
        }
    }
}

#[component]
fn RealAgents() -> Element {
    rsx! {
        section { class: "border-t border-base-300 py-14",
            p { class: "rule-label", "03 / try it with a real agent" }
            div { class: "mt-5 grid gap-4 md:grid-cols-2",
                Step {
                    index: "a",
                    title: "Turn WebMCP on in Chrome",
                    body: rsx! {
                        "Chrome 149 or newer, with "
                        code { "chrome://flags/#enable-webmcp-testing" }
                        " enabled. Then "
                        code { "document.modelContext" }
                        " is real and the polyfill on this page stands down."
                    },
                }
                Step {
                    index: "b",
                    title: "Point an inspector at the page",
                    body: rsx! {
                        "The "
                        a {
                            class: "link link-primary",
                            href: INSPECTOR,
                            rel: "noopener",
                            "Model Context Tool Inspector"
                        }
                        " extension lists a page's tools and calls them, which is the console on this \
                         page done properly."
                    },
                }
                Step {
                    index: "c",
                    title: "Ship it to real users",
                    body: rsx! {
                        "Register for the "
                        a {
                            class: "link link-primary",
                            href: ORIGIN_TRIAL,
                            rel: "noopener",
                            "origin trial"
                        }
                        " and your own origin gets the native API without a flag."
                    },
                }
                Step {
                    index: "d",
                    title: "Everyone else gets the polyfill",
                    body: rsx! {
                        "This page loads Google's "
                        a {
                            class: "link link-primary",
                            href: "https://github.com/GoogleChromeLabs/webmcp-tools",
                            rel: "noopener",
                            "WebMCP polyfill"
                        }
                        " (Apache-2.0) before the wasm starts, so the demo works in any browser. It \
                         returns early when the native API is present."
                    },
                }
            }
        }
    }
}

#[component]
fn Step(index: &'static str, title: &'static str, body: Element) -> Element {
    rsx! {
        div { class: "flex gap-4 rounded border border-base-300 bg-base-200/40 p-5",
            span { class: "font-mono text-sm text-primary/70", "{index}." }
            div {
                h3 { class: "font-medium", "{title}" }
                p { class: "mt-1.5 text-sm leading-relaxed opacity-70", {body} }
            }
        }
    }
}

#[component]
fn TheCode() -> Element {
    rsx! {
        section { id: "code", class: "scroll-mt-16 border-t border-base-300 py-14",
            p { class: "rule-label", "04 / the code" }
            h2 { class: "mt-4 text-3xl font-bold tracking-tight", "Everything the demo does" }
            p { class: "mt-3 max-w-2xl leading-relaxed opacity-70",
                "The whole of "
                code { "site/src/demo.rs" }
                ", included at compile time. Note the rule it exists to demonstrate: tool handlers \
                 are invoked by the browser outside the Dioxus runtime, so they read a plain "
                code { "Rc<RefCell<_>>" }
                " snapshot and send their writes down a coroutine channel."
            }
            pre { class: "mt-6 max-h-[36rem] overflow-auto rounded-lg border border-base-300 bg-base-200/50 p-5 font-mono text-xs leading-relaxed",
                code { "{DEMO_SOURCE}" }
            }
        }
    }
}

#[component]
fn SiteFooter() -> Element {
    rsx! {
        footer { class: "border-t border-base-300",
            div { class: "mx-auto flex w-full max-w-6xl flex-col gap-3 px-5 py-8 font-mono text-xs opacity-55 sm:flex-row sm:items-center sm:justify-between sm:px-8",
                p { "webmcp-rs · MIT" }
                nav { class: "flex flex-wrap gap-5",
                    a { class: "hover:opacity-100", href: REPO, rel: "noopener", "github" }
                    a { class: "hover:opacity-100", href: CRATES_IO, rel: "noopener", "crates.io" }
                    a { class: "hover:opacity-100", href: DOCS_RS, rel: "noopener", "docs.rs" }
                    a { class: "hover:opacity-100", href: SPEC, rel: "noopener", "webmcp spec" }
                }
            }
        }
    }
}
