use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vps_rust::{AppState, Config, app};

#[tokio::test]
async fn health_returns_ok_without_auth() {
    let config = Config {
        api_key: String::new(),
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
    };
    let app = app(AppState::new(config));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), br#"{"status":"ok","version":"0.1.0"}"#);
}
