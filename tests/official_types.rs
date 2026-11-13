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

#[test]
fn official_generated_transport_builds_real_session_and_event_requests() {
    use opencode_api::client::{Auth, Client};
    use opencode_api::generated::{EventSubscribeInput, ProjectID};

    let client = Client::new("http://127.0.0.1:4980")
        .unwrap()
        .auth(Auth::password("secret"));
    let list = SessionListInput {
        project: Some(ProjectID::from("prj_123")),
        parent_id: Some(None),
        limit: Some(50),
        ..Default::default()
    };
    let request = client.prepare(Client::session_list_call(&list).unwrap()).unwrap();
    assert_eq!(request.method, "GET");
    assert_eq!(request.url.path(), "/api/session");
    assert!(request.url.query().unwrap_or("").contains("project=prj_123"));
    assert!(request.url.query().unwrap_or("").contains("parentID=null"));
    assert!(request.headers.iter().any(|(key, value)|
        key == "authorization" && value.starts_with("Basic ")
    ));

    let event = client.prepare(Client::event_subscribe_call(&EventSubscribeInput::default()).unwrap()).unwrap();
    assert_eq!(event.url.path(), "/api/event");
    assert_eq!(event.method, "GET");
}
