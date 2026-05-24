pub mod handlers;
pub mod models;

use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/chat", post(handlers::chat))
        .route("/chat/plan", post(handlers::chat_plan))
        .route("/chat/finalize", post(handlers::chat_finalize))
}
