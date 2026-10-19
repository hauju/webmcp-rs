//! A todo list an agent can drive.
//!
//! This file is rendered verbatim on the page, so it is written to be read. It
//! is the reference for the one rule that catches everyone integrating WebMCP
//! with a UI framework:
//!
//! **Tool handlers run outside the Dioxus runtime.** The browser invokes them
//! from a plain JS callback, with no reactive scope on the stack, so touching a
//! `Signal` inside a handler is a panic waiting to happen. The way around it is
//! a snapshot and a command channel:
//!
//! * **reads** — an `Rc<RefCell<Vec<Todo>>>` snapshot initialized on mount and
//!   refreshed by the coroutine after every mutation. Handlers only ever `borrow()` it.
//! * **writes** — an `UnboundedSender<Request>`. The `use_coroutine` receiver
//!   runs *inside* the runtime and applies mutations to the `Signal`. Tool calls
//!   await a reply sent after the snapshot is current, so a following read sees
//!   the completed mutation.
//!
//! Everything else is ordinary Dioxus: the same list renders for the human, so
//! a tool call and a click land in exactly the same place.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use futures_channel::oneshot;
use futures_util::StreamExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use webmcp::{Tool, ToolResult, use_tool};

/// One item. `Serialize` so `list_todos` can hand the whole list to the agent.
#[derive(Clone, PartialEq, Serialize)]
pub struct Todo {
    pub id: u32,
    pub text: String,
    pub done: bool,
}

/// The only way into the `Signal`. Tool handlers and click handlers both send
/// these; the coroutine applies them.
enum Command {
    Add(String),
    SetDone { id: u32, done: bool },
    Delete(u32),
    ClearCompleted,
}

/// Human actions need no reply; tool calls wait for the applied result.
struct Request {
    command: Command,
    reply: Option<oneshot::Sender<Result<ToolResult, String>>>,
}

impl From<Command> for Request {
    fn from(command: Command) -> Self {
        Self {
            command,
            reply: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tool arguments. `register_typed` derives `inputSchema` from these types, and
// doc comments on the fields become the schema's `description`s — which is what
// the model actually reads when it decides how to fill them in.
// ---------------------------------------------------------------------------

#[derive(Deserialize, JsonSchema)]
struct AddTodo {
    /// The item to add, in the user's own words. Do not add a trailing period.
    text: String,
}

#[derive(Deserialize, JsonSchema)]
struct TodoId {
    /// The `id` of the item to act on, as returned by `list_todos`.
    id: u32,
}

// A tool that takes no arguments still needs an object schema. Plain comment,
// not a doc comment: schemars would put a doc comment in the schema's
// `description`, where the model would read it as guidance.
#[derive(Deserialize, JsonSchema)]
struct NoArgs {}

#[component]
pub fn TodoDemo() -> Element {
    let mut todos = use_signal(|| {
        vec![
            Todo {
                id: 1,
                text: "Read the WebMCP explainer".into(),
                done: true,
            },
            Todo {
                id: 2,
                text: "Register a tool from Rust".into(),
                done: false,
            },
            Todo {
                id: 3,
                text: "Let an agent drive the page".into(),
                done: false,
            },
        ]
    });
    // Initialize before tools are registered, then update in the same turn as
    // each mutation. Reads never depend on a later render/effect being scheduled.
    let snapshot: Rc<RefCell<Vec<Todo>>> = use_hook(|| Rc::new(RefCell::new(todos.peek().clone())));

    let commands = use_coroutine({
        let snapshot = snapshot.clone();
        move |mut rx: UnboundedReceiver<Request>| {
            let snapshot = snapshot.clone();
            async move {
                let mut next_id = 4u32;
                while let Some(Request { command, reply }) = rx.next().await {
                    let result = {
                        let mut list = todos.write();
                        let result = apply(command, &mut list, &mut next_id);
                        *snapshot.borrow_mut() = list.clone();
                        result
                    };
                    // Release the signal borrow and publish the snapshot before
                    // resolving the tool call, even if another call is already queued.
                    if let Some(reply) = reply {
                        let _ = reply.send(result);
                    }
                }
            }
        }
    });
    let tx = commands.tx();

    // -----------------------------------------------------------------------
    // The tools. `use_tool` parks each `ToolHandle` in this component's hook
    // state, so unmounting the demo unregisters all five — no ghost tools
    // answering calls about a list nobody is looking at.
    // -----------------------------------------------------------------------

    use_tool({
        let tx = tx.clone();
        move || {
            Tool::new("add_todo")
                .description(
                    "Add one item to the todo list the user is looking at. \
                     Returns the added item with its id, text and completion state.",
                )
                .register_typed(move |args: AddTodo| {
                    let tx = tx.clone();
                    async move { send(&tx, Command::Add(args.text)).await }
                })
        }
    });

    use_tool({
        let tx = tx.clone();
        move || {
            Tool::new("complete_todo")
                .description(
                    "Mark one todo item as done. Call `list_todos` first if you do not \
                     already know the item's id.",
                )
                .register_typed(move |args: TodoId| {
                    let tx = tx.clone();
                    async move {
                        send(
                            &tx,
                            Command::SetDone {
                                id: args.id,
                                done: true,
                            },
                        )
                        .await
                    }
                })
        }
    });

    use_tool({
        let tx = tx.clone();
        move || {
            Tool::new("delete_todo")
                .description("Remove one todo item from the list for good.")
                .consequential()
                .register_typed(move |args: TodoId| {
                    let tx = tx.clone();
                    async move { send(&tx, Command::Delete(args.id)).await }
                })
        }
    });

    use_tool({
        let snapshot = snapshot.clone();
        move || {
            Tool::new("list_todos")
                .description(
                    "List every todo item with its id, text and completion state. \
                     The other tools take the ids this returns.",
                )
                // Item text is whatever a user typed: data, not instructions.
                .read_only()
                .untrusted_content()
                .register_typed(move |_: NoArgs| {
                    let snapshot = snapshot.clone();
                    async move { Ok::<_, String>(ToolResult::json(&*snapshot.borrow())) }
                })
        }
    });

    use_tool({
        let tx = tx.clone();
        move || {
            Tool::new("clear_completed")
                .description("Remove every item that is already done, leaving the rest alone.")
                .consequential()
                .register_typed(move |_: NoArgs| {
                    let tx = tx.clone();
                    async move { send(&tx, Command::ClearCompleted).await }
                })
        }
    });

    // -----------------------------------------------------------------------
    // The human's half of the same list.
    // -----------------------------------------------------------------------

    let mut draft = use_signal(String::new);
    let items = todos();
    let remaining = items.iter().filter(|todo| !todo.done).count();

    rsx! {
        div { class: "flex h-full flex-col gap-4",
            form {
                class: "flex gap-2",
                onsubmit: {
                    let tx = tx.clone();
                    move |event: FormEvent| {
                        event.prevent_default();
                        let text = draft().trim().to_string();
                        if !text.is_empty() {
                            let _ = tx.unbounded_send(Command::Add(text).into());
                            draft.set(String::new());
                        }
                    }
                },
                input {
                    class: "input input-sm flex-1 font-mono text-xs",
                    r#type: "text",
                    name: "todo",
                    placeholder: "Add an item yourself…",
                    "aria-label": "New todo",
                    value: "{draft}",
                    oninput: move |event| draft.set(event.value()),
                }
                button { class: "btn btn-sm btn-primary", r#type: "submit", "Add" }
            }

            ul { class: "flex flex-col gap-1.5",
                for todo in items.iter().cloned() {
                    li {
                        key: "{todo.id}",
                        class: "rise group flex items-center gap-3 rounded border border-base-300 bg-base-100/60 px-3 py-2",
                        input {
                            class: "checkbox checkbox-xs checkbox-primary",
                            r#type: "checkbox",
                            checked: todo.done,
                            "aria-label": "Toggle {todo.text}",
                            onchange: {
                                let tx = tx.clone();
                                let (id, done) = (todo.id, todo.done);
                                move |_| {
                                    let _ = tx.unbounded_send(Command::SetDone { id, done: !done }.into());
                                }
                            },
                        }
                        span { class: "font-mono text-[0.65rem] text-base-content/35", "#{todo.id}" }
                        span {
                            class: if todo.done { "flex-1 text-sm line-through opacity-45" } else { "flex-1 text-sm" },
                            "{todo.text}"
                        }
                        button {
                            class: "btn btn-ghost btn-xs opacity-0 transition group-hover:opacity-60 hover:!opacity-100",
                            "aria-label": "Delete {todo.text}",
                            onclick: {
                                let tx = tx.clone();
                                let id = todo.id;
                                move |_| {
                                    let _ = tx.unbounded_send(Command::Delete(id).into());
                                }
                            },
                            "✕"
                        }
                    }
                }
                if items.is_empty() {
                    li { class: "rounded border border-dashed border-base-300 px-3 py-6 text-center text-sm opacity-50",
                        "Empty. Ask the console to add something."
                    }
                }
            }

            div { class: "mt-auto flex items-center justify-between border-t border-base-300 pt-3 font-mono text-[0.7rem] text-base-content/50",
                span { "{remaining} open / {items.len()} total" }
                button {
                    class: "link link-hover",
                    onclick: {
                        let tx = tx.clone();
                        move |_| {
                            let _ = tx.unbounded_send(Command::ClearCompleted.into());
                        }
                    },
                    "clear completed"
                }
            }
        }
    }
}

/// Await the applied mutation, not just its delivery to the command queue.
async fn send(tx: &UnboundedSender<Request>, command: Command) -> Result<ToolResult, String> {
    let (reply, response) = oneshot::channel();
    tx.unbounded_send(Request {
        command,
        reply: Some(reply),
    })
    .map_err(|_| "the todo demo is no longer on screen".to_string())?;
    response
        .await
        .map_err(|_| "the todo demo was unmounted before the command completed".to_string())?
}

/// Validate against the current state at the point of mutation. Two queued
/// deletes cannot both succeed, and clear counts reflect what was removed.
fn apply(command: Command, list: &mut Vec<Todo>, next_id: &mut u32) -> Result<ToolResult, String> {
    let missing = |id| format!("no todo has id {id}; call list_todos for the current ids");
    match command {
        Command::Add(text) => {
            let text = text.trim().to_string();
            if text.is_empty() {
                return Err("`text` must not be empty".to_string());
            }
            let id = *next_id;
            *next_id = id.checked_add(1).ok_or("todo ids exhausted")?;
            let todo = Todo {
                id,
                text,
                done: false,
            };
            let result = ToolResult::json(&todo);
            list.push(todo);
            Ok(result)
        }
        Command::SetDone { id, done } => {
            let todo = list
                .iter_mut()
                .find(|todo| todo.id == id)
                .ok_or_else(|| missing(id))?;
            todo.done = done;
            Ok(ToolResult::text(format!(
                "{} \"{}\".",
                if done { "Completed" } else { "Reopened" },
                todo.text
            )))
        }
        Command::Delete(id) => {
            let index = list
                .iter()
                .position(|todo| todo.id == id)
                .ok_or_else(|| missing(id))?;
            let todo = list.remove(index);
            Ok(ToolResult::text(format!("Deleted \"{}\".", todo.text)))
        }
        Command::ClearCompleted => {
            let before = list.len();
            list.retain(|todo| !todo.done);
            Ok(ToolResult::text(match before - list.len() {
                0 => "Nothing was completed, so nothing was cleared.".to_string(),
                1 => "Cleared 1 completed item.".to_string(),
                n => format!("Cleared {n} completed items."),
            }))
        }
    }
}
