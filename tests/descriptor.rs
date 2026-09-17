//! Descriptor-level tests. These run on the host target, where registration
//! always reports `Unsupported` — what they check is everything that happens
//! *before* the browser is involved: schema derivation, result encoding, and
//! the validation that keeps a malformed descriptor from reaching JS.

use webmcp::{Error, Tool, ToolResult};

#[test]
fn tool_results_encode_in_the_protocol_shape() {
    let ok = serde_json::to_value(ToolResult::text("done")).unwrap();
    assert_eq!(ok["content"][0]["type"], "text");
    assert_eq!(ok["content"][0]["text"], "done");
    // Success must not carry isError at all, not `false`.
    assert!(ok.get("isError").is_none());

    let failed = serde_json::to_value(ToolResult::error("nope")).unwrap();
    assert_eq!(failed["isError"], true);
}

#[test]
fn json_results_are_readable_text_blocks() {
    let result = ToolResult::json(&serde_json::json!({ "count": 2 }));
    let encoded = serde_json::to_value(&result).unwrap();
    let text = encoded["content"][0]["text"].as_str().unwrap();

    assert!(text.contains("\"count\": 2"));
    assert!(encoded.get("isError").is_none());
}

#[test]
fn a_non_object_schema_is_rejected_before_it_reaches_the_browser() {
    let err = Tool::new("bad")
        .description("Invalid schema test")
        .input_schema(serde_json::json!("just a string"))
        .register(|_| async { Ok::<_, std::convert::Infallible>(ToolResult::text("")) })
        .unwrap_err();

    assert!(matches!(err, Error::Schema(_)), "got {err:?}");
}

#[test]
fn an_empty_name_is_rejected() {
    let err = Tool::new("  ")
        .description("Invalid name test")
        .register(|_| async { Ok::<_, std::convert::Infallible>(ToolResult::text("")) })
        .unwrap_err();

    assert!(matches!(err, Error::Descriptor(_)), "got {err:?}");
}

#[test]
fn missing_and_blank_descriptions_are_rejected() {
    for description in [None, Some(""), Some(" \n\t")] {
        let mut tool = Tool::new("noop");
        if let Some(description) = description {
            tool = tool.description(description);
        }
        let err = tool
            .register(|_| async { Ok::<_, std::convert::Infallible>(ToolResult::text("")) })
            .unwrap_err();
        assert!(matches!(err, Error::Descriptor(_)), "got {err:?}");
    }
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn registration_off_the_web_is_unsupported_not_a_panic() {
    let err = Tool::new("noop")
        .description("does nothing")
        .register(|_| async { Ok::<_, std::convert::Infallible>(ToolResult::text("")) })
        .unwrap_err();

    assert!(matches!(err, Error::Unsupported), "got {err:?}");
    assert!(!webmcp::is_supported());
}

/// Schema derivation needs `schemars`, which only the `schema` feature pulls in.
#[cfg(feature = "schema")]
mod schema {
    use serde::Deserialize;

    #[derive(Deserialize, schemars::JsonSchema)]
    #[serde(rename_all = "snake_case")]
    // Read by the derived schema, not by this test's own code.
    #[allow(dead_code)]
    struct SearchArgs {
        /// Maximum number of items to return.
        limit: Option<u32>,
        /// Free-text search over the message body.
        search: Option<String>,
    }

    #[test]
    fn derived_schema_describes_the_argument_type() {
        let schema = webmcp::schema_for::<SearchArgs>();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["limit"].is_object());
        assert!(schema["properties"]["search"].is_object());
        // Doc comments become descriptions the model actually reads.
        assert_eq!(
            schema["properties"]["search"]["description"],
            "Free-text search over the message body."
        );
        // Noise the tool descriptor does not want.
        assert!(schema.get("$schema").is_none());
        assert!(schema.get("title").is_none());
    }

    #[test]
    fn a_tool_with_no_arguments_still_declares_an_object_schema() {
        // Agents reject a descriptor whose inputSchema is missing or scalar, so
        // the default must be a real object schema.
        #[derive(Deserialize, schemars::JsonSchema)]
        struct NoArgs {}

        assert_eq!(webmcp::schema_for::<NoArgs>()["type"], "object");
    }
}
