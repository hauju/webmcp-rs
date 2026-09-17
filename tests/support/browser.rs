//! A controlled browser model context. This exercises the actual WASM/JS
//! boundary without depending on a browser's experimental WebMCP flag.
// Shared by the crate and site suites, which use different helper subsets.
#![allow(dead_code)]

use js_sys::{JSON, Promise};
use serde_json::Value;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(inline_js = r#"
export function installContext() {
    const previous = Object.getOwnPropertyDescriptor(document, 'modelContext');
    const tools = new Map();
    const context = new EventTarget();
    context.calls = 0;
    context.registerTool = async (tool, options = {}) => {
        context.calls++;
        if (typeof tool.description !== 'string') throw new TypeError('missing description');
        if (tools.has(tool.name)) throw new Error('duplicate tool');
        if (tool.name === 'rejected') throw new Error('registration denied');
        if (options.signal?.aborted) throw options.signal.reason;
        tools.set(tool.name, tool);
        options.signal?.addEventListener('abort', () => {
            tools.delete(tool.name);
            context.dispatchEvent(new Event('toolchange'));
        }, {once: true});
        context.dispatchEvent(new Event('toolchange'));
        await Promise.resolve();
    };
    context.getTools = async () => Array.from(tools.values());
    context.executeTool = async (tool, args) => {
        const registered = tools.get(tool.name);
        if (!registered) throw new Error('missing tool');
        return registered.execute(args);
    };
    context.restore = () => {
        if (previous) Object.defineProperty(document, 'modelContext', previous);
        else delete document.modelContext;
    };
    Object.defineProperty(document, 'modelContext', {value: context, configurable: true});
    return context;
}
export function restoreContext(context) { context.restore(); }
export async function invokeTool(name, args) {
    const tools = await document.modelContext.getTools();
    const tool = tools.find(tool => tool.name === name);
    if (!tool) throw new Error('missing tool: ' + name);
    return document.modelContext.executeTool(tool, args);
}
export async function registeredTools() { return document.modelContext.getTools(); }
export function undecodableArguments() { return {value: Symbol('invalid JSON')}; }
export function fireToolChange(context) { context.dispatchEvent(new Event('toolchange')); }
"#)]
extern "C" {
    fn installContext() -> JsValue;
    fn restoreContext(context: &JsValue);
    fn invokeTool(name: &str, args: &JsValue) -> Promise;
    fn registeredTools() -> Promise;
    pub fn undecodableArguments() -> JsValue;
    pub fn fireToolChange(context: &JsValue);
}

pub struct Context(pub JsValue);

impl Context {
    pub fn new() -> Self {
        Self(installContext())
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        restoreContext(&self.0);
    }
}

pub fn json(value: &JsValue) -> Value {
    serde_json::from_str(&String::from(JSON::stringify(value).unwrap())).unwrap()
}

pub async fn invoke(name: &str, args: JsValue) -> Value {
    json(&JsFuture::from(invokeTool(name, &args)).await.unwrap())
}

pub async fn names() -> Vec<String> {
    json(&JsFuture::from(registeredTools()).await.unwrap())
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect()
}

pub async fn descriptors() -> Value {
    json(&JsFuture::from(registeredTools()).await.unwrap())
}
