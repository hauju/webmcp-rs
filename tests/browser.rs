#![cfg(target_family = "wasm")]

#[path = "support/browser.rs"]
mod browser;

use js_sys::{JSON, Reflect};
use std::{cell::Cell, convert::Infallible, rc::Rc};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use webmcp::{Error, Tool, ToolResult};

wasm_bindgen_test_configure!(run_in_browser);

fn tool(name: &str) -> Tool {
    Tool::new(name).description("Browser integration test")
}

// One scenario owns the document context so async tests cannot replace each
// other's browser global. Helpers below cover each public registration API.
#[wasm_bindgen_test]
async fn browser_registration_and_execution() {
    let context = browser::Context::new();
    assert!(webmcp::is_supported());
    let missing = Tool::new("missing")
        .register(|_| async { Ok::<_, Infallible>(ToolResult::text("unexpected")) });
    assert!(matches!(missing, Err(Error::Descriptor(_))));
    assert_eq!(
        Reflect::get(&context.0, &"calls".into()).unwrap().as_f64(),
        Some(0.0)
    );

    let calls = Rc::new(Cell::new(0));
    let handler_calls = calls.clone();
    let handle = tool("echo")
        .register(move |args| {
            handler_calls.set(handler_calls.get() + 1);
            async move { Ok::<_, Infallible>(ToolResult::json(&args)) }
        })
        .unwrap();
    assert_eq!(handle.name(), "echo");
    assert_eq!(browser::names().await, ["echo"]);
    assert_eq!(
        browser::descriptors().await[0]["inputSchema"]["type"],
        "object"
    );
    let result = browser::invoke("echo", JSON::parse(r#"{"text":"hello"}"#).unwrap()).await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(result["content"][0]["text"].as_str().unwrap())
            .unwrap()["text"],
        "hello"
    );
    assert!(result.get("isError").is_none());
    let empty = browser::invoke("echo", JsValue::UNDEFINED).await;
    assert_eq!(empty["content"][0]["text"], "{}");
    assert_eq!(calls.get(), 2);

    for args in [
        browser::undecodableArguments(),
        JsValue::NULL,
        JSON::parse("[]").unwrap(),
        42.into(),
    ] {
        let failed = browser::invoke("echo", args).await;
        assert_eq!(failed["isError"], true);
        assert!(
            failed["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("invalid arguments")
        );
    }
    assert_eq!(calls.get(), 2, "invalid inputs must not call the handler");

    let duplicate = tool("echo")
        .register_async(|_| async { Ok::<_, Infallible>(ToolResult::text("wrong")) })
        .await;
    assert!(matches!(duplicate, Err(Error::Rejected(message)) if message.contains("duplicate")));
    assert_eq!(
        browser::names().await,
        ["echo"],
        "rejecting a duplicate must preserve the original"
    );
    drop(handle);
    assert!(browser::names().await.is_empty(), "drop must unregister");

    let rejected = tool("rejected")
        .register_async(|_| async { Ok::<_, Infallible>(ToolResult::text("wrong")) })
        .await;
    assert!(matches!(rejected, Err(Error::Rejected(message)) if message.contains("denied")));

    let handle = tool("failure")
        .register_async(|_| async { Err::<ToolResult, _>("handler failed") })
        .await
        .unwrap();
    let result = browser::invoke("failure", JsValue::UNDEFINED).await;
    assert_eq!(result["isError"], true);
    assert_eq!(result["content"][0]["text"], "handler failed");
    drop(handle);

    descriptor_metadata().await;
    cancellation().await;

    #[cfg(feature = "schema")]
    typed_registration().await;
    #[cfg(feature = "dioxus")]
    component_lifetime().await;
    #[cfg(feature = "leptos")]
    owner_lifetime().await;
    assert!(browser::names().await.is_empty());
}

async fn descriptor_metadata() {
    let plain = tool("plain")
        .register(|_| async { Ok::<_, Infallible>(ToolResult::text("")) })
        .unwrap();
    let annotated = tool("annotated")
        .title("Annotated tool")
        .read_only()
        .untrusted_content()
        .register(|_| async { Ok::<_, Infallible>(ToolResult::text("")) })
        .unwrap();
    let descriptors = browser::descriptors().await;
    let find = |name: &str| {
        descriptors
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == name)
            .unwrap()
            .clone()
    };
    // Hints are opt-in: nothing set means no `annotations` key, not `false`s.
    let plain_descriptor = find("plain");
    assert!(plain_descriptor.get("annotations").is_none());
    assert!(plain_descriptor.get("title").is_none());
    let annotated_descriptor = find("annotated");
    assert_eq!(annotated_descriptor["title"], "Annotated tool");
    assert_eq!(
        annotated_descriptor["annotations"],
        serde_json::json!({ "readOnlyHint": true, "untrustedContentHint": true })
    );
    drop((plain, annotated));
}

async fn cancellation() {
    struct Dropped(Rc<Cell<bool>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    let dropped = Rc::new(Cell::new(false));
    let flag = dropped.clone();
    let handle = tool("forever")
        .register(move |_| {
            let guard = Dropped(flag.clone());
            async move {
                std::future::pending::<()>().await;
                drop(guard);
                Ok::<_, Infallible>(ToolResult::text("unreachable"))
            }
        })
        .unwrap();
    let result = browser::invoke_then_cancel("forever", JsValue::UNDEFINED).await;
    assert_eq!(result["isError"], true);
    assert_eq!(result["content"][0]["text"], "cancelled");
    assert!(dropped.get(), "cancelling must drop the handler's future");
    drop(handle);
}

#[cfg(feature = "schema")]
async fn typed_registration() {
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct Args {
        text: String,
    }

    let handle = tool("typed")
        .register_typed_async(|args: Args| async move {
            Ok::<_, Infallible>(ToolResult::text(args.text))
        })
        .await
        .unwrap();
    let descriptor = browser::descriptors().await;
    assert_eq!(
        descriptor[0]["inputSchema"]["properties"]["text"]["type"],
        "string"
    );
    let ok = browser::invoke("typed", JSON::parse(r#"{"text":"typed hello"}"#).unwrap()).await;
    assert_eq!(ok["content"][0]["text"], "typed hello");
    let failed = browser::invoke("typed", JSON::parse("{}").unwrap()).await;
    assert_eq!(failed["isError"], true);
    assert!(
        failed["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("text")
    );
    let duplicate = tool("typed")
        .register_typed_async(|_: Args| async { Ok::<_, Infallible>(ToolResult::text("wrong")) })
        .await;
    assert!(matches!(duplicate, Err(Error::Rejected(_))));
    drop(handle);

    let handle = tool("typed_sync")
        .register_typed(
            |args: Args| async move { Ok::<_, Infallible>(ToolResult::text(args.text)) },
        )
        .unwrap();
    assert_eq!(
        browser::invoke("typed_sync", JSON::parse("{}").unwrap()).await["isError"],
        true
    );
    drop(handle);
}

#[cfg(feature = "dioxus")]
async fn component_lifetime() {
    fn app() -> dioxus::prelude::Element {
        webmcp::use_tool(|| {
            tool("mounted").register(|_| async { Ok::<_, Infallible>(ToolResult::text("mounted")) })
        });
        VNode::empty()
    }
    use dioxus::prelude::*;
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    assert_eq!(browser::names().await, ["mounted"]);
    drop(dom);
    assert!(browser::names().await.is_empty());
}

#[cfg(feature = "leptos")]
async fn owner_lifetime() {
    use leptos::prelude::Owner;

    let owner = Owner::new();
    owner.with(|| {
        webmcp::leptos::use_tool(|| {
            tool("owned").register(|_| async { Ok::<_, Infallible>(ToolResult::text("owned")) })
        });
    });
    assert_eq!(browser::names().await, ["owned"]);
    // Cleaning up the owner is what unmounting a component does.
    drop(owner);
    assert!(browser::names().await.is_empty());
}
