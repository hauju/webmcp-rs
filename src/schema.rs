//! Deriving `inputSchema` from a Rust type.

use serde_json::Value;

/// JSON Schema for `T`, trimmed to what a tool descriptor wants.
///
/// `schemars` emits a `$schema` dialect declaration and a `title` taken from the
/// type name. Both are noise in a tool descriptor — the dialect is fixed by the
/// protocol, and a `title` of `ListFeedbackArgs` tells the model nothing the
/// tool's own description does not — so they are stripped.
pub fn schema_for<T: schemars::JsonSchema>() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(T))
        .unwrap_or_else(|_| Value::Object(Default::default()));

    if let Some(object) = schema.as_object_mut() {
        object.remove("$schema");
        object.remove("title");
        // A struct always generates `"type": "object"`, but a type alias or a
        // newtype might not; agents require an object at the top level.
        object
            .entry("type")
            .or_insert_with(|| Value::String("object".into()));
    }

    schema
}
