//! A todo list an agent can drive.
//!
//! This file is rendered verbatim on the page, so it is written to be read. It
//! is the reference for the one rule that catches everyone integrating WebMCP
//! with a UI framework:
//!
//! **Tool handlers run outside the Dioxus runtime.** The browser invokes them
//! from a plain JS callback, with no reactive scope on the stack, so touching a
//! `Signal` inside a handler is a panic waiting to happen. The way around it is
//! two one-way channels:
//!
//! * **reads** — an `Rc<RefCell<Vec<Todo>>>` snapshot the component refreshes
//!   after every render. Handlers only ever `borrow()` it.
//! * **writes** — an `UnboundedSender<Command>`. The `use_coroutine` receiver
//!   runs *inside* the runtime and is the only thing that touches the `Signal`.
//!
//! Everything else is ordinary Dioxus: the same list renders for the human, so
//! a tool call and a click land in exactly the same place.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
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
    let mut next_id = use_signal(|| 4u32);

    // The read side. A handler cannot read `todos`, so the component keeps this
    // plain snapshot in step with it after every render.
    let snapshot: Rc<RefCell<Vec<Todo>>> = use_hook(|| Rc::new(RefCell::new(Vec::new())));
    use_effect({
        let snapshot = snapshot.clone();
        move || *snapshot.borrow_mut() = todos()
    });

    // The write side. This closure is the only code that touches the `Signal`,
    // and it runs inside the runtime because the coroutine owns it.
    let commands = use_coroutine(move |mut rx: UnboundedReceiver<Command>| async move {
        while let Some(command) = rx.next().await {
            match command {
                Command::Add(text) => {
                    let id = next_id();
                    next_id.set(id + 1);
                    todos.write().push(Todo {
                        id,
                        text,
                        done: false,
                    });
                }
                Command::SetDone { id, done } => {
                    let mut list = todos.write();
                    if let Some(todo) = list.iter_mut().find(|todo| todo.id == id) {
                        todo.done = done;
                    }
                }
                Command::Delete(id) => todos.write().retain(|todo| todo.id != id),
                Command::ClearCompleted => todos.write().retain(|todo| !todo.done),
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
                     Returns the text that was added.",
                )
                .register_typed(move |args: AddTodo| {
                    let tx = tx.clone();
                    async move {
                        let text = args.text.trim().to_string();
                        if text.is_empty() {
                            return Err("`text` must not be empty".to_string());
                        }
                        send(&tx, Command::Add(text.clone()))?;
                        Ok(ToolResult::text(format!("Added \"{text}\".")))
                    }
                })
        }
    });

    use_tool({
        let tx = tx.clone();
        let snapshot = snapshot.clone();
        move || {
            Tool::new("complete_todo")
                .description(
                    "Mark one todo item as done. Call `list_todos` first if you do not \
                     already know the item's id.",
                )
                .register_typed(move |args: TodoId| {
                    let tx = tx.clone();
                    let snapshot = snapshot.clone();
                    async move {
                        let text = find(&snapshot, args.id)?;
                        send(
                            &tx,
                            Command::SetDone {
                                id: args.id,
                                done: true,
                            },
                        )?;
                        Ok::<_, String>(ToolResult::text(format!("Completed \"{text}\".")))
                    }
                })
        }
    });

    use_tool({
        let tx = tx.clone();
        let snapshot = snapshot.clone();
        move || {
            Tool::new("delete_todo")
                .description("Remove one todo item from the list for good.")
                .register_typed(move |args: TodoId| {
                    let tx = tx.clone();
                    let snapshot = snapshot.clone();
                    async move {
                        let text = find(&snapshot, args.id)?;
                        send(&tx, Command::Delete(args.id))?;
                        Ok::<_, String>(ToolResult::text(format!("Deleted \"{text}\".")))
                    }
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
                .register_typed(move |_: NoArgs| {
                    let snapshot = snapshot.clone();
                    async move { Ok::<_, String>(ToolResult::json(&*snapshot.borrow())) }
                })
        }
    });

    use_tool({
        let tx = tx.clone();
        let snapshot = snapshot.clone();
        move || {
            Tool::new("clear_completed")
                .description("Remove every item that is already done, leaving the rest alone.")
                .register_typed(move |_: NoArgs| {
                    let tx = tx.clone();
                    let snapshot = snapshot.clone();
                    async move {
                        let cleared = snapshot.borrow().iter().filter(|todo| todo.done).count();
                        send(&tx, Command::ClearCompleted)?;
                        Ok::<_, String>(ToolResult::text(match cleared {
                            0 => "Nothing was completed, so nothing was cleared.".to_string(),
                            1 => "Cleared 1 completed item.".to_string(),
                            n => format!("Cleared {n} completed items."),
                        }))
                    }
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
                            let _ = tx.unbounded_send(Command::Add(text));
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
                                    let _ = tx.unbounded_send(Command::SetDone { id, done: !done });
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
                                    let _ = tx.unbounded_send(Command::Delete(id));
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
                            let _ = tx.unbounded_send(Command::ClearCompleted);
                        }
                    },
                    "clear completed"
                }
            }
        }
    }
}

/// Push a mutation at the coroutine. The only way this fails is the demo having
/// been unmounted, which the agent should hear about rather than see swallowed.
fn send(tx: &UnboundedSender<Command>, command: Command) -> Result<(), String> {
    tx.unbounded_send(command)
        .map_err(|_| "the todo demo is no longer on screen".to_string())
}

/// Resolve an id against the read snapshot, so a bad id comes back to the agent
/// as a message it can act on instead of a silent no-op.
fn find(snapshot: &Rc<RefCell<Vec<Todo>>>, id: u32) -> Result<String, String> {
    snapshot
        .borrow()
        .iter()
        .find(|todo| todo.id == id)
        .map(|todo| todo.text.clone())
        .ok_or_else(|| format!("no todo has id {id}; call list_todos for the current ids"))
}
