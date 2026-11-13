//! Can a GPUI client consume official OpenCode Rust schema types without OpenTUI?
//! This is a dependency compatibility test, not a replacement for real service testing.
use opencode_api_types::{
    ModelRef, ProviderID, SessionMessageInfo, SessionsResponse, SessionListInput,
};
use serde_json::json;

#[test]
fn official_model_reference_preserves_provider_and_variant() {
    let model: ModelRef = serde_json::from_value(json!({
        "id":"gpt-6", "providerID":"openai", "variant":"high"
    })).unwrap();
    assert_eq!(model.id, "gpt-6");
    assert_eq!(model.provider_id, ProviderID::from("openai"));
    assert_eq!(model.variant.as_deref(), Some("high"));
    assert_eq!(serde_json::to_value(model).unwrap()["providerID"], "openai");
}

#[test]
fn official_transcript_keeps_future_unknown_message_types() {
    let event = json!({"id":"msg_future","type":"future-message","custom":"preserve"});
    let parsed: SessionMessageInfo = serde_json::from_value(event.clone()).unwrap();
    assert!(matches!(parsed, SessionMessageInfo::Unknown(_)));
    assert_eq!(serde_json::to_value(parsed).unwrap(), event);
}

#[test]
fn official_v2_session_list_understands_cursor_response() {
    let response: SessionsResponse = serde_json::from_value(json!({
        "data":[],
        "cursor":{"next":"opaque-next"}
    })).unwrap();
    assert!(response.data.is_empty());
    assert!(response.cursor.next.is_some());
    let _input = SessionListInput::default();
}
