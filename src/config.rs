use std::{env, net::SocketAddr};

#[derive(Clone, Debug)]
pub struct Config {
    pub api_key: String,
    pub host: String,
    pub port: u16,
    pub version: String,
    pub openrouter_api_key: String,
    pub openrouter_base_url: String,
    pub openrouter_llm_model: String,
    pub docs_path: String,
    pub rag_top_k: usize,
    pub context_policy_path: String,
    pub sf_login_url: String,
    pub sf_client_id: String,
    pub sf_username: String,
    pub sf_private_key_path: String,
    pub sf_api_version: String,
    pub discord_bot_token: Option<String>,
    pub discord_command_prefix: String,
    pub discord_allowed_guild_ids: Vec<u64>,
    pub discord_allowed_channel_ids: Vec<u64>,
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_u64_list_env(name: &str) -> Vec<u64> {
    env::var(name)
        .ok()
        .map(|value| parse_u64_list(&value))
        .unwrap_or_default()
}

pub fn parse_u64_list(value: &str) -> Vec<u64> {
    value
        .split([',', ' ', '\n', '\t'])
        .filter_map(|item| item.trim().parse().ok())
        .collect()
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            api_key: env::var("API_KEY").unwrap_or_default(),
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8000),
            version: env!("CARGO_PKG_VERSION").to_string(),
            openrouter_api_key: env::var("OPENROUTER_API_KEY").unwrap_or_default(),
            openrouter_base_url: env::var("OPENROUTER_BASE_URL")
                .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string()),
            openrouter_llm_model: env::var("OPENROUTER_LLM_MODEL")
                .unwrap_or_else(|_| "qwen/qwen3-235b-a22b-2507".to_string()),
            docs_path: env::var("DOCS_PATH").unwrap_or_else(|_| "./docs".to_string()),
            rag_top_k: env::var("RAG_TOP_K")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(5),
            context_policy_path: env::var("CONTEXT_POLICY_PATH")
                .unwrap_or_else(|_| "./contexts.json".to_string()),
            sf_login_url: env::var("SF_LOGIN_URL")
                .unwrap_or_else(|_| "https://login.salesforce.com".to_string()),
            sf_client_id: env::var("SF_CLIENT_ID").unwrap_or_default(),
            sf_username: env::var("SF_USERNAME").unwrap_or_default(),
            sf_private_key_path: env::var("SF_PRIVATE_KEY_PATH")
                .unwrap_or_else(|_| "./keys/sf_private.pem".to_string()),
            sf_api_version: env::var("SF_API_VERSION").unwrap_or_else(|_| "v61.0".to_string()),
            discord_bot_token: optional_env("DISCORD_BOT_TOKEN"),
            discord_command_prefix: env::var("DISCORD_COMMAND_PREFIX")
                .unwrap_or_else(|_| "!agente".to_string()),
            discord_allowed_guild_ids: parse_u64_list_env("DISCORD_ALLOWED_GUILD_IDS"),
            discord_allowed_channel_ids: parse_u64_list_env("DISCORD_ALLOWED_CHANNEL_IDS"),
        }
    }

    pub fn socket_addr(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        format!("{}:{}", self.host, self.port).parse()
    }
}
