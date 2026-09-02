//! An agent console: list the page's tools, fill in arguments, call one.
//!
//! This is scaffolding for the demo, not crate API. `webmcp-rs` is the *page*
//! side of WebMCP — it hands tools out. Discovering and calling them is the
//! agent's job, done here by hand through `js_sys` so a visitor whose browser
//! has no agent can still watch the round trip.

use std::rc::Rc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use js_sys::{Array, Function, JSON, Object, Promise, Reflect};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// One tool as the page advertises it.
#[derive(Clone, PartialEq)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

/// What the console found on `document.modelContext`.
#[derive(Clone, PartialEq)]
enum Discovery {
    Loading,
    /// The context exists but is missing the discovery half of the API.
    Unsupported(String),
    Tools(Vec<ToolInfo>),
}

/// The console and the tool list sit in different parts of the page but show
/// the same reading, and there is exactly one of each — a global is simpler
/// than threading a signal through the layout.
static DISCOVERY: GlobalSignal<Discovery> = Signal::global(|| Discovery::Loading);

#[component]
pub fn AgentConsole() -> Element {
    let mut selected = use_signal(String::new);
    let mut arguments = use_signal(|| "{}".to_string());
    let mut output = use_signal(|| None::<Result<String, String>>);
    let mut running = use_signal(|| false);

    // `toolchange` fires from the browser, outside the Dioxus runtime, so the
    // listener pokes this coroutine instead of writing a signal itself.
    let refresh = use_coroutine(move |mut rx: UnboundedReceiver<()>| async move {
        loop {
            match list_tools().await {
                Ok(tools) => {
                    // Keep the visitor's selection if it survived the change.
                    if !tools.iter().any(|tool| tool.name == selected()) {
                        match tools.first() {
                            Some(first) => {
                                selected.set(first.name.clone());
                                arguments.set(prefill(&first.schema));
                            }
                            None => selected.set(String::new()),
                        }
                    }
                    *DISCOVERY.write() = Discovery::Tools(tools);
                }
                Err(why) => *DISCOVERY.write() = Discovery::Unsupported(why),
            }
            if rx.next().await.is_none() {
                break;
            }
        }
    });

    // Parked in hook state so the closure outlives the listener registration.
    use_hook(move || {
        let context = model_context()?;
        let add = Reflect::get(&context, &JsValue::from_str("addEventListener"))
            .ok()?
            .dyn_into::<Function>()
            .ok()?;
        let tx = refresh.tx();
        let listener = Closure::<dyn FnMut()>::new(move || {
            let _ = tx.unbounded_send(());
        });
        add.call2(
            &context,
            &JsValue::from_str("toolchange"),
            listener.as_ref(),
        )
        .ok()?;
        // `Closure` is not `Clone`, which `use_hook` requires, so it rides in an `Rc`.
        Some(Rc::new(listener))
    });

    let state = DISCOVERY();
    let tools = match &state {
        Discovery::Tools(tools) => tools.clone(),
        _ => Vec::new(),
    };
    let described = tools.clone();
    let description = tools
        .iter()
        .find(|tool| tool.name == selected())
        .map(|tool| tool.description.clone());

    let result = output();
    let failed = matches!(result, Some(Err(_)));
    let result_text = match result {
        Some(Ok(text) | Err(text)) => Some(text),
        None => None,
    };

    rsx! {
        div { class: "flex h-full flex-col gap-3",
            if matches!(state, Discovery::Loading) {
                p { class: "font-mono text-xs opacity-50", "Reading document.modelContext…" }
            }

            if let Discovery::Unsupported(why) = &state {
                div { class: "rounded border border-warning/40 bg-warning/10 p-3 text-xs",
                    p { class: "font-mono text-warning", "Console unavailable" }
                    p { class: "mt-1 opacity-70", "{why}" }
                    p { class: "mt-2 opacity-70",
                        "The tools are still registered. An agent that only calls "
                        code { "executeTool" }
                        " can use them."
                    }
                }
            }

            if !tools.is_empty() {
                label { class: "flex flex-col gap-1",
                    span { class: "rule-label", "tool" }
                    select {
                        class: "select select-sm font-mono text-xs",
                        name: "tool",
                        value: "{selected}",
                        onchange: move |event| {
                            let name = event.value();
                            if let Some(tool) = described.iter().find(|tool| tool.name == name) {
                                arguments.set(prefill(&tool.schema));
                            }
                            selected.set(name);
                            output.set(None);
                        },
                        for tool in tools.iter() {
                            option { key: "{tool.name}", value: "{tool.name}", "{tool.name}" }
                        }
                    }
                }

                if let Some(description) = description {
                    p { class: "text-xs leading-relaxed opacity-60", "{description}" }
                }

                label { class: "flex flex-col gap-1",
                    span { class: "rule-label", "arguments (json)" }
                    textarea {
                        class: "textarea textarea-sm min-h-24 w-full font-mono text-xs leading-relaxed",
                        name: "arguments",
                        spellcheck: false,
                        value: "{arguments}",
                        oninput: move |event| arguments.set(event.value()),
                    }
                }

                button {
                    class: "btn btn-sm btn-primary self-start",
                    disabled: running(),
                    onclick: move |_| async move {
                        match serde_json::from_str::<Value>(&arguments()) {
                            Err(error) => {
                                output.set(Some(Err(format!("arguments are not valid JSON: {error}"))))
                            }
                            Ok(args) => {
                                running.set(true);
                                let called = execute(&selected(), args).await;
                                output.set(Some(called));
                                running.set(false);
                            }
                        }
                    },
                    if running() {
                        span { class: "loading loading-spinner loading-xs" }
                        "calling…"
                    } else {
                        "Run tool"
                    }
                }
            }

            if let Some(text) = result_text {
                div { class: "flex flex-col gap-1",
                    span { class: "rule-label", "result" }
                    pre {
                        class: if failed { "max-h-56 overflow-auto rounded border border-error/40 bg-base-100 p-3 font-mono text-[0.7rem] leading-relaxed text-error" } else { "max-h-56 overflow-auto rounded border border-primary/30 bg-base-100 p-3 font-mono text-[0.7rem] leading-relaxed" },
                        "{text}"
                    }
                }
            }
        }
    }
}

/// The same reading as the console, rendered as the page's tool inventory.
#[component]
pub fn RegisteredTools() -> Element {
    rsx! {
        match DISCOVERY() {
            Discovery::Loading => rsx! {
                p { class: "font-mono text-xs opacity-50", "Reading…" }
            },
            Discovery::Unsupported(why) => rsx! {
                p { class: "font-mono text-xs opacity-60", "{why}" }
            },
            Discovery::Tools(tools) => rsx! {
                div { class: "grid gap-3 md:grid-cols-2 xl:grid-cols-3",
                    for tool in tools.iter() {
                        div {
                            key: "{tool.name}",
                            // `min-w-0`: without it the grid track grows to the
                            // widest schema line instead of letting the `pre` scroll.
                            class: "flex min-w-0 flex-col gap-2 rounded border border-base-300 bg-base-200/40 p-4",
                            code { class: "text-sm font-medium text-primary", "{tool.name}" }
                            p { class: "text-xs leading-relaxed opacity-65", "{tool.description}" }
                            pre { class: "overflow-x-auto rounded bg-base-100 p-2 font-mono text-[0.65rem] leading-relaxed opacity-70",
                                {serde_json::to_string_pretty(&tool.schema).unwrap_or_default()}
                            }
                        }
                    }
                }
            },
        }
    }
}

// ---------------------------------------------------------------------------
// The `document.modelContext` side. Reflective, like the crate itself: a
// browser with a different surface degrades to a message instead of throwing.
// ---------------------------------------------------------------------------

fn model_context() -> Option<Object> {
    let document = web_sys::window()?.document()?;
    Reflect::get(&document, &JsValue::from_str("modelContext"))
        .ok()?
        .dyn_into::<Object>()
        .ok()
}

fn method(context: &Object, name: &str) -> Option<Function> {
    Reflect::get(context, &JsValue::from_str(name))
        .ok()?
        .dyn_into::<Function>()
        .ok()
}

/// Await a value the spec says is a promise but an implementation might hand
/// back directly.
async fn settle(value: JsValue) -> Result<JsValue, JsValue> {
    match value.dyn_into::<Promise>() {
        Ok(promise) => JsFuture::from(promise).await,
        Err(plain) => Ok(plain),
    }
}

async fn raw_tools() -> Result<(Object, Array), String> {
    let context = model_context().ok_or("this page has no document.modelContext")?;
    let get_tools = method(&context, "getTools")
        .ok_or("this model context has no getTools(), so tools cannot be listed")?;
    let returned = get_tools.call0(&context).map_err(describe)?;
    let settled = settle(returned).await.map_err(describe)?;
    Ok((context, Array::from(&settled)))
}

async fn list_tools() -> Result<Vec<ToolInfo>, String> {
    let (_, array) = raw_tools().await?;
    Ok(array.iter().map(describe_tool).collect())
}

fn describe_tool(entry: JsValue) -> ToolInfo {
    let string = |key: &str| {
        Reflect::get(&entry, &JsValue::from_str(key))
            .ok()
            .and_then(|value| value.as_string())
            .unwrap_or_default()
    };
    let schema = Reflect::get(&entry, &JsValue::from_str("inputSchema"))
        .ok()
        .and_then(|value| JSON::stringify(&value).ok())
        .and_then(|text| serde_json::from_str(&String::from(text)).ok())
        .unwrap_or(Value::Null);

    ToolInfo {
        name: string("name"),
        description: string("description"),
        schema,
    }
}

/// Call one tool by name and pretty-print whatever comes back.
async fn execute(name: &str, args: Value) -> Result<String, String> {
    let (context, array) = raw_tools().await?;
    // `executeTool` takes the tool *object* the page handed out, not its name,
    // so the list is re-read rather than cached.
    let tool = array
        .iter()
        .find(|entry| {
            Reflect::get(entry, &JsValue::from_str("name"))
                .ok()
                .and_then(|value| value.as_string())
                .as_deref()
                == Some(name)
        })
        .ok_or_else(|| format!("`{name}` is no longer registered"))?;

    let execute_tool = method(&context, "executeTool")
        .ok_or("this model context has no executeTool(), so tools cannot be called")?;
    let encoded = JSON::parse(&args.to_string()).map_err(describe)?;
    let returned = execute_tool
        .call2(&context, &tool, &encoded)
        .map_err(describe)?;
    let settled = settle(returned).await.map_err(describe)?;

    Ok(pretty(&settled))
}

fn pretty(value: &JsValue) -> String {
    JSON::stringify_with_replacer_and_space(value, &JsValue::NULL, &JsValue::from_f64(2.0))
        .ok()
        .map(String::from)
        .unwrap_or_else(|| format!("{value:?}"))
}

fn describe(error: JsValue) -> String {
    if let Some(text) = error.as_string() {
        return text;
    }
    Reflect::get(&error, &JsValue::from_str("message"))
        .ok()
        .and_then(|message| message.as_string())
        .unwrap_or_else(|| pretty(&error))
}

/// Seed the argument box with one key per schema property, so the visitor edits
/// values instead of remembering field names.
fn prefill(schema: &Value) -> String {
    let mut object = serde_json::Map::new();
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, property) in properties {
            object.insert(name.clone(), placeholder(property));
        }
    }
    serde_json::to_string_pretty(&Value::Object(object)).unwrap_or_else(|_| "{}".to_string())
}

fn placeholder(property: &Value) -> Value {
    match property.get("type").and_then(Value::as_str) {
        Some("string") => json!(""),
        Some("integer" | "number") => json!(0),
        Some("boolean") => json!(false),
        Some("array") => json!([]),
        Some("object") => json!({}),
        _ => Value::Null,
    }
}
