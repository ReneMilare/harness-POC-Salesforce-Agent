use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{Config, agent::sessions::SessionMessage};

#[derive(Clone, Debug)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug)]
pub enum LlmError {
    MissingApiKey,
    Request(String),
    Api(StatusCode, String),
    EmptyResponse,
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingApiKey => write!(formatter, "OPENROUTER_API_KEY não configurada"),
            Self::Request(error) => write!(formatter, "falha na chamada OpenRouter: {error}"),
            Self::Api(status, body) => write!(formatter, "OpenRouter retornou {status}: {body}"),
            Self::EmptyResponse => write!(formatter, "OpenRouter não retornou conteúdo"),
        }
    }
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<OpenRouterMessage>,
    temperature: f32,
}

#[derive(Serialize)]
struct OpenRouterMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

pub async fn complete(config: &Config, messages: Vec<LlmMessage>) -> Result<String, LlmError> {
    if config.openrouter_api_key.trim().is_empty() {
        return Err(LlmError::MissingApiKey);
    }

    let request = ChatCompletionRequest {
        model: &config.openrouter_llm_model,
        messages: messages
            .into_iter()
            .map(|message| OpenRouterMessage {
                role: message.role,
                content: message.content,
            })
            .collect(),
        temperature: 0.2,
    };

    let url = format!(
        "{}/chat/completions",
        config.openrouter_base_url.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(url)
        .bearer_auth(&config.openrouter_api_key)
        .header("HTTP-Referer", "https://agente-salesforce-poc.local")
        .header("X-Title", "Agente Salesforce POC")
        .json(&request)
        .send()
        .await
        .map_err(|error| LlmError::Request(error.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(LlmError::Api(status, body));
    }

    let body = response
        .json::<ChatCompletionResponse>()
        .await
        .map_err(|error| LlmError::Request(error.to_string()))?;

    body.choices
        .into_iter()
        .find_map(|choice| choice.message.content)
        .filter(|content| !content.trim().is_empty())
        .ok_or(LlmError::EmptyResponse)
}

pub fn session_to_messages(history: &[SessionMessage]) -> Vec<LlmMessage> {
    history
        .iter()
        .map(|message| LlmMessage {
            role: match message.role.as_str() {
                "assistant" => "assistant".to_string(),
                _ => "user".to_string(),
            },
            content: message.content.clone(),
        })
        .collect()
}
