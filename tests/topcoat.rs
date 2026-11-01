//! The Topcoat bridge: what the script carries and how calls dispatch.
#![cfg(feature = "topcoat")]

use serde_json::{Value, json};
use topcoat::{
    context::CxTestBuilder,
    view::{ViewExt, view},
};
use webmcp::{
    Tool, ToolResult,
    topcoat::{ToolCall, webmcp_tools},
};

async fn render(tools: &[Tool], endpoint: &str) -> topcoat::Result<String> {
    let cx = &CxTestBuilder::new().build();
    let html = view! { cx => webmcp_tools(tools: tools, endpoint: endpoint) }
        .single()
        .await?
        .render(cx);
    Ok(html)
}

/// The `(endpoint, tools)` arguments the script is invoked with.
fn script_args(html: &str) -> (Value, Value) {
    let body = html
        .strip_prefix("<script>")
        .and_then(|s| s.strip_suffix(");</script>"))
        .unwrap_or_else(|| panic!("not a single script: {html}"));
    let args = &body[body.rfind("})(").expect("invocation") + 3..];
    let Value::Array(mut args) = serde_json::from_str(&format!("[{args}]")).unwrap() else {
        unreachable!()
    };
    let tools = args.pop().unwrap();
    (args.pop().unwrap(), tools)
}

fn add_todo() -> Tool {
    Tool::new("add_todo")
        .title("Add todo")
        .description("Add an item to the user's todo list")
        .consequential()
        .exposed_to(["https://agent.example"])
}

#[tokio::test]
async fn script_carries_descriptors_and_endpoint() {
    let html = render(&[add_todo()], "/webmcp").await.unwrap();
    let (endpoint, tools) = script_args(&html);
    assert_eq!(endpoint, "/webmcp");
    assert_eq!(
        tools,
        json!([{
            "name": "add_todo",
            "title": "Add todo",
            "description": "Add an item to the user's todo list",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "consequentialHint": true },
            "exposedTo": ["https://agent.example"],
        }])
    );
}

#[tokio::test]
async fn markup_in_descriptors_cannot_close_the_script() {
    let tool = Tool::new("x").description("</script><script>alert(1)</script><!--");
    let html = render(&[tool], "/a</script>").await.unwrap();
    assert_eq!(html.matches("</script>").count(), 1, "{html}");
    assert!(!html.contains("<!--"));
    let (endpoint, tools) = script_args(&html);
    assert_eq!(endpoint, "/a</script>");
    assert_eq!(
        tools[0]["description"],
        "</script><script>alert(1)</script><!--"
    );
}

#[tokio::test]
async fn invalid_descriptor_fails_the_render() {
    assert!(
        render(&[Tool::new("no_description")], "/webmcp")
            .await
            .is_err()
    );
}

#[derive(serde::Deserialize)]
struct AddTodo {
    text: String,
}

fn call(body: Value) -> ToolCall {
    serde_json::from_value(body).unwrap()
}

fn json_of(result: ToolResult) -> Value {
    serde_json::to_value(result).unwrap()
}

#[tokio::test]
async fn run_decodes_arguments() {
    let call = call(json!({ "name": "add_todo", "arguments": { "text": "milk" } }));
    let result = call
        .run(|args: AddTodo| async move {
            Ok::<_, std::convert::Infallible>(ToolResult::text(format!("Added {}", args.text)))
        })
        .await;
    assert_eq!(
        json_of(result),
        json!({ "content": [{ "type": "text", "text": "Added milk" }] })
    );
}

#[tokio::test]
async fn bad_arguments_and_handler_errors_are_error_results() {
    let bad = call(json!({ "name": "add_todo", "arguments": { "txt": "milk" } }))
        .run(|_: AddTodo| async { Ok::<_, String>(ToolResult::text("unreachable")) })
        .await;
    assert!(bad.is_error);

    let failed = call(json!({ "name": "add_todo", "arguments": { "text": "milk" } }))
        .run(|_: AddTodo| async { Err::<ToolResult, _>("list is full") })
        .await;
    assert_eq!(
        json_of(failed),
        json!({ "content": [{ "type": "text", "text": "list is full" }], "isError": true })
    );
}

#[derive(serde::Deserialize)]
struct NoArgs {}

#[tokio::test]
async fn missing_arguments_decode_as_empty_object() {
    let result = call(json!({ "name": "list" }))
        .run(|_: NoArgs| async { Ok::<_, String>(ToolResult::text("ok")) })
        .await;
    assert!(!result.is_error);
}

#[test]
fn unknown_names_the_tool() {
    let result = call(json!({ "name": "nope" })).unknown();
    assert!(result.is_error);
    assert_eq!(json_of(result)["content"][0]["text"], "unknown tool `nope`");
}
