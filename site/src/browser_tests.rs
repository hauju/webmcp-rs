#[path = "../../tests/support/browser.rs"]
mod browser;

use dioxus::prelude::*;
use futures_util::{
    future::{Either, select},
    pin_mut,
};
use js_sys::JSON;
use serde_json::Value;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// Drive Dioxus tasks while awaiting an external agent's call. The call itself
// runs outside the framework runtime, just as it does in a real browser.
async fn call(dom: &mut VirtualDom, name: &str, args: &str) -> Value {
    let result = browser::invoke(name, JSON::parse(args).unwrap());
    pin_mut!(result);
    loop {
        match select(&mut result, Box::pin(dom.wait_for_work())).await {
            Either::Left((result, _)) => return result,
            Either::Right(_) => {}
        }
        dom.render_immediate_to_vec();
    }
}

fn body(result: &Value) -> Value {
    assert!(result.get("isError").is_none(), "{result}");
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[wasm_bindgen_test]
async fn demo_mutations_are_visible_when_the_call_returns() {
    let _context = browser::Context::new();
    let mut dom = VirtualDom::new(crate::demo::TodoDemo);
    dom.rebuild_in_place();
    assert_eq!(browser::names().await.len(), 5);

    // Registration is immediately readable, without waiting for an effect.
    assert_eq!(
        body(&call(&mut dom, "list_todos", "{}").await)
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let added = body(&call(&mut dom, "add_todo", r#"{"text":"  immediate read  "}"#).await);
    assert_eq!(added["text"], "immediate read");
    let id = added["id"].as_u64().unwrap();
    let items = body(&call(&mut dom, "list_todos", "{}").await);
    assert!(items.as_array().unwrap().contains(&added));

    let args = format!(r#"{{"id":{id}}}"#);
    let completed = call(&mut dom, "complete_todo", &args).await;
    assert!(completed.get("isError").is_none());
    let items = body(&call(&mut dom, "list_todos", "{}").await);
    assert_eq!(
        items
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == id)
            .unwrap()["done"],
        true
    );
    let cleared = call(&mut dom, "clear_completed", "{}").await;
    assert_eq!(cleared["content"][0]["text"], "Cleared 2 completed items.");
    assert_eq!(
        body(&call(&mut dom, "list_todos", "{}").await)
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(call(&mut dom, "delete_todo", &args).await["isError"], true);

    let deleted = call(&mut dom, "delete_todo", r#"{"id":2}"#).await;
    assert!(deleted.get("isError").is_none());
    assert_eq!(
        call(&mut dom, "delete_todo", r#"{"id":2}"#).await["isError"],
        true
    );
    assert_eq!(
        call(&mut dom, "add_todo", r#"{"text":"  "}"#).await["isError"],
        true
    );
    assert_eq!(
        body(&call(&mut dom, "list_todos", "{}").await)
            .as_array()
            .unwrap()
            .len(),
        1
    );
    drop(dom);
    assert!(browser::names().await.is_empty());
}
