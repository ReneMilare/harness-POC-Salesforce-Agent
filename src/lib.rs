pub mod agent;
pub mod api;
pub mod config;
pub mod context_policy;
pub mod discord;
pub mod routing;
pub mod salesforce;

use axum::Router;
use std::sync::Arc;

use agent::sessions::SessionStore;
pub use config::Config;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub sessions: Arc<SessionStore>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            sessions: Arc::new(SessionStore::new()),
        }
    }

    pub async fn chat(&self, session_id: &str, user_message: &str) -> String {
        let history = self.sessions.get_history(session_id);
        let response_text = agent::graph::run(&self.config, &history, user_message).await;
        self.append_turn(session_id, user_message, &response_text);

        response_text
    }

    pub async fn discord_chat(&self, session_id: &str, user_message: &str) -> String {
        let history = self.sessions.get_history(session_id);

        if let Some(response_text) =
            salesforce::answer_account_question(&self.config, &history, user_message).await
        {
            self.append_turn(session_id, user_message, &response_text);
            return response_text;
        }

        let response_text = agent::graph::run(&self.config, &history, user_message).await;
        self.append_turn(session_id, user_message, &response_text);

        response_text
    }

    fn append_turn(&self, session_id: &str, user_message: &str, assistant_message: &str) {
        self.sessions
            .append_message(session_id, "user", user_message);
        self.sessions
            .append_message(session_id, "assistant", assistant_message);
    }
}

pub fn app(state: AppState) -> Router {
    Router::new().merge(api::router()).with_state(state)
}
