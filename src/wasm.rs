//! The browser side: raw access to `modelContext` and the registration itself.
//!
//! There are no `web-sys` bindings for WebMCP yet, so the interface is reached
//! reflectively. That is deliberate rather than lazy: the shape of this API
//! moved twice in 2026 (`provideContext()`/`clearContext()` removed in March,
//! `unregisterTool()` replaced by an `AbortSignal` in April), and a reflective
//! lookup degrades to [`Error::Unsupported`] on a browser that has a different
//! or older surface, where a hard `extern` binding would throw.

use std::rc::Rc;

use js_sys::{Array, Function, Object, Promise, Reflect};
use serde::Serialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, future_to_promise, spawn_local};
use web_sys::AbortController;

use crate::error::{Error, Result};
use crate::tool::{ErasedHandler, Tool, ToolHandle, validate_schema};

/// Resolve the page's model context.
///
/// `document.modelContext` is the current home; `navigator.modelContext` is the
/// deprecated alias earlier drafts (and shipped origin trials) used, so it is
/// tried second rather than dropped.
pub fn model_context() -> Option<Object> {
    let window = web_sys::window()?;
    let hosts: [Option<JsValue>; 2] = [
        window.document().map(JsValue::from),
        Some(JsValue::from(window.navigator())),
    ];
    for host in hosts.into_iter().flatten() {
        if let Ok(ctx) = Reflect::get(&host, &JsValue::from_str("modelContext"))
            && ctx.is_object()
        {
            return ctx.dyn_into::<Object>().ok();
        }
    }
    None
}

/// Whether this page can register tools.
pub fn is_supported() -> bool {
    model_context().is_some()
}

/// Owns the browser-side lifetime of one registration.
pub struct Registration {
    controller: AbortController,
    /// The `execute` callback must outlive the registration — dropping it while
    /// the browser still holds a reference makes the tool throw when called.
    _execute: Closure<dyn FnMut(JsValue) -> Promise>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        // Aborting the signal is what unregisters the tool under the current
        // spec; there is no unregisterTool() to call.
        self.controller.abort();
    }
}

/// Build the descriptor, the abort controller and the `execute` closure.
fn prepare(tool: &Tool, handler: ErasedHandler) -> Result<(Object, Object, Registration)> {
    validate_schema(tool)?;

    // `json_compatible()`, not `to_value()`: the default serializer turns a
    // `serde_json::Value::Object` into a JS `Map`, and a `Map` stringifies to
    // `{}` — so every agent would plan against an empty schema.
    let schema = tool
        .input_schema
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| Error::Schema(e.to_string()))?;

    let execute = Closure::<dyn FnMut(JsValue) -> Promise>::new(move |args: JsValue| {
        let handler = Rc::clone(&handler);
        future_to_promise(async move {
            // A tool called with no arguments arrives as `undefined`; treat that
            // as an empty object so handlers taking all-optional parameters work.
            let parsed = if args.is_undefined() || args.is_null() {
                serde_json::Value::Object(Default::default())
            } else {
                serde_wasm_bindgen::from_value(args)
                    .unwrap_or(serde_json::Value::Object(Default::default()))
            };
            let result = handler(parsed).await;
            serde_wasm_bindgen::to_value(&result)
                .map_err(|e| JsValue::from_str(&format!("failed to encode tool result: {e}")))
        })
    });

    let descriptor = Object::new();
    set(&descriptor, "name", &JsValue::from_str(&tool.name))?;
    if let Some(description) = &tool.description {
        set(&descriptor, "description", &JsValue::from_str(description))?;
    }
    set(&descriptor, "inputSchema", &schema)?;
    set(&descriptor, "execute", execute.as_ref())?;

    let controller = AbortController::new().map_err(|e| Error::Rejected(describe(&e)))?;
    let options = Object::new();
    set(&options, "signal", &controller.signal())?;
    if !tool.exposed_to.is_empty() {
        let origins = Array::new();
        for origin in &tool.exposed_to {
            origins.push(&JsValue::from_str(origin));
        }
        set(&options, "exposedTo", &origins)?;
    }

    Ok((
        descriptor,
        options,
        Registration {
            controller,
            _execute: execute,
        },
    ))
}

fn call_register(descriptor: &Object, options: &Object) -> Result<Promise> {
    let ctx = model_context().ok_or(Error::Unsupported)?;
    let register = Reflect::get(&ctx, &JsValue::from_str("registerTool"))
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
        .ok_or(Error::Unsupported)?;

    let returned = register
        .call2(&ctx, descriptor, options)
        .map_err(|e| Error::Rejected(describe(&e)))?;

    // The spec returns a promise; tolerate a synchronous implementation.
    Ok(returned
        .dyn_into::<Promise>()
        .unwrap_or_else(|_| Promise::resolve(&JsValue::UNDEFINED)))
}

pub fn register(tool: Tool, handler: ErasedHandler) -> Result<ToolHandle> {
    let (descriptor, options, registration) = prepare(&tool, handler)?;
    let promise = call_register(&descriptor, &options)?;

    // Registration is async, so a rejection (most often a duplicate name)
    // cannot reach this return value. Surface it on the console instead of
    // swallowing it — a tool that silently never appears is the worst outcome.
    let name = tool.name.clone();
    spawn_local(async move {
        if let Err(e) = JsFuture::from(promise).await {
            crate::warn(&format!(
                "webmcp: registerTool(\"{name}\") was rejected: {}",
                describe(&e)
            ));
        }
    });

    Ok(ToolHandle {
        name: tool.name,
        _registration: registration,
    })
}

pub async fn register_async(tool: Tool, handler: ErasedHandler) -> Result<ToolHandle> {
    let (descriptor, options, registration) = prepare(&tool, handler)?;
    let promise = call_register(&descriptor, &options)?;

    JsFuture::from(promise)
        .await
        .map_err(|e| Error::Rejected(describe(&e)))?;

    Ok(ToolHandle {
        name: tool.name,
        _registration: registration,
    })
}

fn set(target: &Object, key: &str, value: &JsValue) -> Result<()> {
    Reflect::set(target, &JsValue::from_str(key), value)
        .map(|_| ())
        .map_err(|e| Error::Rejected(describe(&e)))
}

/// Render a thrown JS value as text. `Error` objects stringify to something
/// useful; anything else falls back to its debug form.
fn describe(value: &JsValue) -> String {
    if let Some(text) = value.as_string() {
        return text;
    }
    Reflect::get(value, &JsValue::from_str("message"))
        .ok()
        .and_then(|m| m.as_string())
        .unwrap_or_else(|| format!("{value:?}"))
}
