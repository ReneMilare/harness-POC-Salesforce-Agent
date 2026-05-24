use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vps_rust::{AppState, Config, app};

fn test_state() -> AppState {
    AppState::new(Config {
        api_key: "test-key-123".to_string(),
        host: "127.0.0.1".to_string(),
        port: 0,
        version: "0.1.0".to_string(),
        openrouter_api_key: String::new(),
        openrouter_base_url: "https://openrouter.ai/api/v1".to_string(),
        openrouter_llm_model: "test-model".to_string(),
        docs_path: "./missing-test-docs".to_string(),
        rag_top_k: 5,
        context_policy_path: "./missing-contexts.json".to_string(),
        sf_login_url: "https://login.salesforce.com".to_string(),
        sf_client_id: String::new(),
        sf_username: String::new(),
        sf_private_key_path: "./keys/sf_private.pem".to_string(),
        sf_api_version: "v61.0".to_string(),
        discord_bot_token: None,
        discord_command_prefix: "!agente".to_string(),
        discord_allowed_guild_ids: Vec::new(),
        discord_allowed_channel_ids: Vec::new(),
    })
}

fn chat_request(api_key: Option<&str>, session_id: &str, message: &str) -> Request<Body> {
    let body = serde_json::json!({
        "session_id": session_id,
        "message": message,
        "user_id": "user-001"
    });

    let mut builder = Request::builder()
        .method("POST")
        .uri("/chat")
        .header("content-type", "application/json");

    if let Some(api_key) = api_key {
        builder = builder.header("X-API-Key", api_key);
    }

    builder.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn chat_without_api_key_returns_403() {
    let app = app(test_state());

    let response = app.oneshot(chat_request(None, "s1", "oi")).await.unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn chat_with_wrong_api_key_returns_403() {
    let app = app(test_state());

    let response = app
        .oneshot(chat_request(Some("chave-errada"), "s1", "oi"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn chat_returns_session_id_and_response() {
    let app = app(test_state());

    let response = app
        .oneshot(chat_request(Some("test-key-123"), "sess-abc", "olá"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["session_id"], "sess-abc");
    assert!(json["response"].as_str().unwrap().contains("olá"));
}

#[tokio::test]
async fn chat_multi_turn_accumulates_history() {
    let state = test_state();
    let app = app(state.clone());

    app.clone()
        .oneshot(chat_request(
            Some("test-key-123"),
            "sess-multi",
            "primeira pergunta",
        ))
        .await
        .unwrap();

    app.oneshot(chat_request(
        Some("test-key-123"),
        "sess-multi",
        "segunda pergunta",
    ))
    .await
    .unwrap();

    let history = state.sessions.get_history("sess-multi");
    assert_eq!(history.len(), 4);
    assert_eq!(history[0].role, "user");
    assert_eq!(history[0].content, "primeira pergunta");
    assert_eq!(history[2].content, "segunda pergunta");
}

#[tokio::test]
async fn chat_sessions_are_isolated() {
    let state = test_state();
    let app = app(state.clone());

    app.clone()
        .oneshot(chat_request(Some("test-key-123"), "sessao-a", "msg A"))
        .await
        .unwrap();

    app.oneshot(chat_request(Some("test-key-123"), "sessao-b", "msg B"))
        .await
        .unwrap();

    let history_b = state.sessions.get_history("sessao-b");
    assert_eq!(history_b.len(), 2);
    assert_eq!(history_b[0].content, "msg B");
}
