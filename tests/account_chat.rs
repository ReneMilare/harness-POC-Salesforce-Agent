use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use std::{fs, time::SystemTime};
use tower::ServiceExt;
use vps_rust::{AppState, Config, app};

fn test_state() -> AppState {
    test_state_with_docs("./missing-test-docs")
}

fn test_state_with_docs(docs_path: &str) -> AppState {
    test_state_with_docs_and_policy(docs_path, "./missing-contexts.json")
}

fn test_state_with_docs_and_policy(docs_path: &str, context_policy_path: &str) -> AppState {
    AppState::new(Config {
        api_key: "test-key-123".to_string(),
        host: "127.0.0.1".to_string(),
        port: 0,
        version: "0.1.0".to_string(),
        openrouter_api_key: String::new(),
        openrouter_base_url: "https://openrouter.ai/api/v1".to_string(),
        openrouter_llm_model: "test-model".to_string(),
        docs_path: docs_path.to_string(),
        rag_top_k: 5,
        context_policy_path: context_policy_path.to_string(),
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

fn account_fields() -> serde_json::Value {
    serde_json::json!([
        {"api_name": "Id", "label": "Account ID", "data_type": "id"},
        {"api_name": "Name", "label": "Nome da conta", "data_type": "string"},
        {"api_name": "Phone", "label": "Telefone", "data_type": "phone"},
        {"api_name": "Website", "label": "Site", "data_type": "url"}
    ])
}

fn post_json(path: &str, body: serde_json::Value, api_key: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");

    if let Some(api_key) = api_key {
        builder = builder.header("X-API-Key", api_key);
    }

    builder.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn account_plan_requires_api_key() {
    let request = serde_json::json!({
        "session_id": "s1",
        "message": "telefone da conta Acme",
        "user_id": "005",
        "account_fields": account_fields()
    });

    let response = app(test_state())
        .oneshot(post_json("/chat/plan", request, None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn account_plan_returns_account_intent() {
    let request = serde_json::json!({
        "session_id": "s1",
        "message": "qual o telefone da conta Acme Brasil?",
        "user_id": "005",
        "account_fields": account_fields()
    });

    let response = app(test_state())
        .oneshot(post_json("/chat/plan", request, Some("test-key-123")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["session_id"], "s1");
    assert!(json["response"].is_null());
    assert_eq!(json["account_intent"]["object"], "Account");
    assert_eq!(json["account_intent"]["operation"], "get_or_list");
    assert_eq!(json["account_intent"]["limit"], 5);
    assert!(
        json["account_intent"]["requested_fields"]
            .as_array()
            .unwrap()
            .contains(&serde_json::Value::String("Phone".to_string()))
    );
}

#[tokio::test]
async fn account_plan_rejects_contact_object_request() {
    let request = serde_json::json!({
        "session_id": "s1",
        "message": "e quem é o contato?",
        "user_id": "005",
        "account_fields": account_fields()
    });

    let response = app(test_state())
        .oneshot(post_json("/chat/plan", request, Some("test-key-123")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["account_intent"].is_null());
    assert!(
        json["response"]
            .as_str()
            .unwrap()
            .contains("somente dados do objeto Account")
    );
}

#[tokio::test]
async fn account_plan_rejects_general_knowledge_request() {
    let request = serde_json::json!({
        "session_id": "s1",
        "message": "Qual a capital do Canadá?",
        "user_id": "005",
        "account_fields": account_fields()
    });

    let response = app(test_state())
        .oneshot(post_json("/chat/plan", request, Some("test-key-123")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["account_intent"].is_null());
    assert!(
        json["response"]
            .as_str()
            .unwrap()
            .contains("somente informações do objeto Account")
    );
}

#[tokio::test]
async fn account_plan_allows_local_document_request() {
    let root = temp_docs_dir("account_plan_allows_local_document_request");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("apologia_socrates_pt.txt"),
        "Sócrates defende sua conduta diante dos atenienses.",
    )
    .unwrap();
    let request = serde_json::json!({
        "session_id": "s1",
        "message": "fale sobre Socrates",
        "user_id": "005",
        "account_fields": account_fields()
    });

    let response = app(test_state_with_docs(root.to_str().unwrap()))
        .oneshot(post_json("/chat/plan", request, Some("test-key-123")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["account_intent"].is_null());
    assert!(json["response"].as_str().unwrap().contains("Sócrates"));

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn account_plan_allows_socrates_trial_question_from_local_docs() {
    let root = temp_docs_dir("account_plan_allows_socrates_trial_question_from_local_docs");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("apologia_socrates_pt.txt"),
        "No julgamento, Sócrates foi acusado de corromper a juventude e de não reconhecer os deuses da cidade.",
    )
    .unwrap();
    let request = serde_json::json!({
        "session_id": "s1",
        "message": "qual foi a acusação de Sócrates no seu julgamento?",
        "user_id": "005",
        "account_fields": account_fields()
    });

    let response = app(test_state_with_docs(root.to_str().unwrap()))
        .oneshot(post_json("/chat/plan", request, Some("test-key-123")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["account_intent"].is_null());
    assert!(
        json["response"]
            .as_str()
            .unwrap()
            .contains("corromper a juventude")
    );

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn account_plan_allows_socrates_sentence_question_from_local_docs() {
    let root = temp_docs_dir("account_plan_allows_socrates_sentence_question_from_local_docs");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("apologia_socrates_pt.txt"),
        "A sentença de Sócrates foi a condenação à morte, executada pela ingestão de cicuta.",
    )
    .unwrap();
    let request = serde_json::json!({
        "session_id": "s1",
        "message": "Qual foi a sentença de Sócrates?",
        "user_id": "005",
        "account_fields": account_fields()
    });

    let response = app(test_state_with_docs(root.to_str().unwrap()))
        .oneshot(post_json("/chat/plan", request, Some("test-key-123")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["account_intent"].is_null());
    assert!(
        json["response"]
            .as_str()
            .unwrap()
            .contains("condenação à morte")
    );

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn account_plan_blocks_disabled_local_docs_context() {
    let root = temp_docs_dir("account_plan_blocks_disabled_local_docs_context");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("apologia.txt"), "Sócrates aparece no documento.").unwrap();
    let policy_path = root.join("contexts.json");
    fs::write(&policy_path, policy_json(["account"], "Contexto bloqueado")).unwrap();
    let request = serde_json::json!({
        "session_id": "s1",
        "message": "fale sobre Socrates",
        "user_id": "005",
        "account_fields": account_fields()
    });

    let response = app(test_state_with_docs_and_policy(
        root.to_str().unwrap(),
        policy_path.to_str().unwrap(),
    ))
    .oneshot(post_json("/chat/plan", request, Some("test-key-123")))
    .await
    .unwrap();

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["account_intent"].is_null());
    assert_eq!(json["response"], "Contexto bloqueado");

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn account_plan_blocks_disabled_account_context() {
    let root = temp_docs_dir("account_plan_blocks_disabled_account_context");
    fs::create_dir_all(&root).unwrap();
    let policy_path = root.join("contexts.json");
    fs::write(
        &policy_path,
        policy_json(["local_docs"], "Account bloqueado"),
    )
    .unwrap();
    let request = serde_json::json!({
        "session_id": "s1",
        "message": "telefone da conta Acme",
        "user_id": "005",
        "account_fields": account_fields()
    });

    let response = app(test_state_with_docs_and_policy(
        root.to_str().unwrap(),
        policy_path.to_str().unwrap(),
    ))
    .oneshot(post_json("/chat/plan", request, Some("test-key-123")))
    .await
    .unwrap();

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["account_intent"].is_null());
    assert_eq!(json["response"], "Account bloqueado");

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn account_finalize_formats_records_without_llm() {
    let request = serde_json::json!({
        "session_id": "s1",
        "message": "qual o telefone da conta Acme Brasil?",
        "user_id": "005",
        "account_intent": {
            "object": "Account",
            "operation": "get_or_list",
            "requested_fields": ["Id", "Name", "Phone"],
            "filters": [{"field": "Name", "operator": "contains", "value": "Acme Brasil"}],
            "limit": 5
        },
        "account_result": {
            "status": "ok",
            "records": [{"Id": "001000000000001AAA", "Name": "Acme Brasil", "Phone": "11999990000"}],
            "errors": [],
            "has_more": false
        }
    });

    let response = app(test_state())
        .oneshot(post_json("/chat/finalize", request, Some("test-key-123")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let text = json["response"].as_str().unwrap();

    assert!(text.contains("Acme Brasil"));
    assert!(text.contains("11999990000"));
}

fn temp_docs_dir(test_name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{test_name}_{suffix}"))
}

fn policy_json<const N: usize>(contexts: [&str; N], blocked_message: &str) -> String {
    let contexts = contexts
        .iter()
        .map(|context| format!("\"{context}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{
            "allowed_contexts": [{contexts}],
            "blocked_salesforce_objects": [
                {{"api_name": "Contact", "aliases": ["contact", "contato"]}}
            ],
            "messages": {{
                "blocked_context": "{blocked_message}",
                "blocked_salesforce_object": "Objeto bloqueado"
            }}
        }}"#
    )
}
