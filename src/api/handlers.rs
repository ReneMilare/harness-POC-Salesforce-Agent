use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;

use crate::{
    AppState,
    agent::{
        account_intent::{self, PlanOutcome},
        graph,
        guardrails::check_output,
    },
    api::models::{
        ChatFinalizeRequest, ChatPlanRequest, ChatPlanResponse, ChatRequest, ChatResponse,
    },
};

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    version: String,
}

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: state.config.version,
    })
}

#[derive(Serialize)]
pub struct ErrorResponse {
    detail: &'static str,
}

pub async fn chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    verify_api_key(&state, &headers)?;

    let response_text = state.chat(&body.session_id, &body.message).await;

    Ok(Json(ChatResponse {
        session_id: body.session_id,
        response: response_text,
    }))
}

pub async fn chat_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ChatPlanRequest>,
) -> Result<Json<ChatPlanResponse>, (StatusCode, Json<ErrorResponse>)> {
    verify_api_key(&state, &headers)?;

    let history = state.sessions.get_history(&body.session_id);
    let outcome =
        account_intent::plan(&state.config, &history, &body.message, &body.account_fields).await;

    match outcome {
        PlanOutcome::DirectResponse(response_text) => {
            append_turn(&state, &body.session_id, &body.message, &response_text);
            Ok(Json(ChatPlanResponse {
                session_id: body.session_id,
                response: Some(response_text),
                account_intent: None,
            }))
        }
        PlanOutcome::NormalResponse(_) => {
            let response_text = graph::run(&state.config, &history, &body.message).await;
            append_turn(&state, &body.session_id, &body.message, &response_text);
            Ok(Json(ChatPlanResponse {
                session_id: body.session_id,
                response: Some(response_text),
                account_intent: None,
            }))
        }
        PlanOutcome::AccountIntent(intent) => {
            state
                .sessions
                .append_message(&body.session_id, "user", &body.message);
            Ok(Json(ChatPlanResponse {
                session_id: body.session_id,
                response: None,
                account_intent: Some(intent),
            }))
        }
    }
}

pub async fn chat_finalize(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ChatFinalizeRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    verify_api_key(&state, &headers)?;

    let response = account_intent::finalize(
        &state.config,
        &body.message,
        &body.account_intent,
        &body.account_result,
    )
    .await;
    let (_, safe_response) = check_output(&response);

    state
        .sessions
        .append_message(&body.session_id, "assistant", &safe_response);

    Ok(Json(ChatResponse {
        session_id: body.session_id,
        response: safe_response,
    }))
}

fn verify_api_key(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let key = headers
        .get("X-API-Key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if key.is_empty() || key != state.config.api_key {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                detail: "API key inválida",
            }),
        ));
    }

    Ok(())
}

fn append_turn(state: &AppState, session_id: &str, user_message: &str, assistant_message: &str) {
    state
        .sessions
        .append_message(session_id, "user", user_message);
    state
        .sessions
        .append_message(session_id, "assistant", assistant_message);
}
