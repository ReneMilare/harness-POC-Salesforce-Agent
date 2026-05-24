use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
};

use serenity::{
    async_trait,
    model::{
        channel::Message,
        gateway::{GatewayIntents, Ready},
    },
    prelude::*,
};

use crate::AppState;

const DISCORD_MESSAGE_LIMIT: usize = 1900;

#[derive(Clone, Debug)]
pub struct DiscordConfig {
    bot_token: String,
    command_prefix: String,
    allowed_guild_ids: HashSet<u64>,
    allowed_channel_ids: HashSet<u64>,
}

impl DiscordConfig {
    fn from_state(state: &AppState) -> Option<Self> {
        Some(Self {
            bot_token: state.config.discord_bot_token.clone()?,
            command_prefix: state.config.discord_command_prefix.clone(),
            allowed_guild_ids: state
                .config
                .discord_allowed_guild_ids
                .iter()
                .copied()
                .collect(),
            allowed_channel_ids: state
                .config
                .discord_allowed_channel_ids
                .iter()
                .copied()
                .collect(),
        })
    }
}

pub async fn run(state: AppState) -> Result<(), serenity::Error> {
    let Some(discord_config) = DiscordConfig::from_state(&state) else {
        tracing::info!("DISCORD_BOT_TOKEN ausente; bot Discord desativado");
        return Ok(());
    };

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;
    let bot_user_id = Arc::new(RwLock::new(None));
    let handler = DiscordHandler {
        state,
        config: discord_config.clone(),
        bot_user_id: bot_user_id.clone(),
    };

    tracing::info!(
        prefix = %discord_config.command_prefix,
        allowed_guilds = discord_config.allowed_guild_ids.len(),
        allowed_channels = discord_config.allowed_channel_ids.len(),
        "iniciando bot Discord"
    );

    Client::builder(discord_config.bot_token, intents)
        .event_handler(handler)
        .await?
        .start()
        .await
}

struct DiscordHandler {
    state: AppState,
    config: DiscordConfig,
    bot_user_id: Arc<RwLock<Option<u64>>>,
}

#[async_trait]
impl EventHandler for DiscordHandler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        *self
            .bot_user_id
            .write()
            .expect("discord bot id lock poisoned") = Some(ready.user.id.get());
        tracing::info!(bot = %ready.user.name, bot_id = ready.user.id.get(), "bot Discord conectado");
    }

    async fn message(&self, ctx: Context, message: Message) {
        if message.author.bot || !self.is_allowed(&message) {
            return;
        }

        let bot_user_id = *self
            .bot_user_id
            .read()
            .expect("discord bot id lock poisoned");
        let Some(user_message) =
            discord_user_message(&message.content, &self.config.command_prefix, bot_user_id)
        else {
            return;
        };

        let session_id = discord_session_id(
            message.guild_id.map(|guild_id| guild_id.get()),
            message.channel_id.get(),
            message.author.id.get(),
        );
        tracing::info!(
            channel_id = message.channel_id.get(),
            user_id = message.author.id.get(),
            "mensagem Discord aceita"
        );
        let response_text = self.state.discord_chat(&session_id, &user_message).await;

        for chunk in split_discord_message(&response_text) {
            if let Err(error) = message.channel_id.say(&ctx.http, chunk).await {
                tracing::warn!(
                    %error,
                    channel_id = message.channel_id.get(),
                    "falha ao responder mensagem Discord"
                );
                break;
            }
        }
    }
}

impl DiscordHandler {
    fn is_allowed(&self, message: &Message) -> bool {
        let guild_allowed = self.config.allowed_guild_ids.is_empty()
            || message
                .guild_id
                .map(|guild_id| self.config.allowed_guild_ids.contains(&guild_id.get()))
                .unwrap_or(false);
        let channel_allowed = self.config.allowed_channel_ids.is_empty()
            || self
                .config
                .allowed_channel_ids
                .contains(&message.channel_id.get());

        guild_allowed && channel_allowed
    }
}

pub fn message_without_prefix(content: &str, prefix: &str) -> Option<String> {
    let content = content.trim();
    let prefix = prefix.trim();

    if prefix.is_empty() {
        return (!content.is_empty()).then(|| content.to_string());
    }

    content
        .strip_prefix(prefix)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(ToString::to_string)
}

pub fn discord_user_message(
    content: &str,
    prefix: &str,
    bot_user_id: Option<u64>,
) -> Option<String> {
    message_without_prefix(content, prefix).or_else(|| {
        bot_user_id.and_then(|bot_user_id| message_without_mention(content, bot_user_id))
    })
}

pub fn message_without_mention(content: &str, bot_user_id: u64) -> Option<String> {
    let content = content.trim();
    let mentions = [format!("<@{bot_user_id}>"), format!("<@!{bot_user_id}>")];

    mentions.into_iter().find_map(|mention| {
        content.strip_prefix(&mention).and_then(|message| {
            let message = message
                .trim_start()
                .trim_start_matches([',', ':', '-'])
                .trim();
            (!message.is_empty()).then(|| message.to_string())
        })
    })
}

pub fn discord_session_id(guild_id: Option<u64>, channel_id: u64, user_id: u64) -> String {
    match guild_id {
        Some(guild_id) => format!("discord:guild:{guild_id}:channel:{channel_id}:user:{user_id}"),
        None => format!("discord:dm:channel:{channel_id}:user:{user_id}"),
    }
}

pub fn split_discord_message(content: &str) -> Vec<String> {
    let content = content.trim();
    if content.is_empty() {
        return vec!["Não consegui gerar uma resposta.".to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in content.lines() {
        if line.len() > DISCORD_MESSAGE_LIMIT {
            flush_chunk(&mut chunks, &mut current);
            split_long_line(line, &mut chunks);
            continue;
        }

        let separator_len = usize::from(!current.is_empty());
        if current.len() + separator_len + line.len() > DISCORD_MESSAGE_LIMIT {
            flush_chunk(&mut chunks, &mut current);
        }

        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }

    flush_chunk(&mut chunks, &mut current);
    chunks
}

fn split_long_line(line: &str, chunks: &mut Vec<String>) {
    let mut current = String::new();

    for character in line.chars() {
        if current.len() + character.len_utf8() > DISCORD_MESSAGE_LIMIT {
            chunks.push(current);
            current = String::new();
        }
        current.push(character);
    }

    if !current.is_empty() {
        chunks.push(current);
    }
}

fn flush_chunk(chunks: &mut Vec<String>, current: &mut String) {
    if !current.trim().is_empty() {
        chunks.push(std::mem::take(current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_u64_list;

    #[test]
    fn message_without_prefix_requires_non_empty_command_body() {
        assert_eq!(
            message_without_prefix("!agente olá", "!agente"),
            Some("olá".to_string())
        );
        assert_eq!(message_without_prefix("!agente", "!agente"), None);
        assert_eq!(message_without_prefix("olá", "!agente"), None);
    }

    #[test]
    fn discord_user_message_accepts_bot_mention() {
        assert_eq!(
            discord_user_message(
                "<@1508205663350685706> quanto tempo durou?",
                "!agente",
                Some(1508205663350685706)
            ),
            Some("quanto tempo durou?".to_string())
        );
        assert_eq!(
            discord_user_message(
                "<@!1508205663350685706>: olá",
                "!agente",
                Some(1508205663350685706)
            ),
            Some("olá".to_string())
        );
    }

    #[test]
    fn discord_session_id_includes_discord_scope() {
        assert_eq!(
            discord_session_id(Some(10), 20, 30),
            "discord:guild:10:channel:20:user:30"
        );
        assert_eq!(
            discord_session_id(None, 20, 30),
            "discord:dm:channel:20:user:30"
        );
    }

    #[test]
    fn split_discord_message_keeps_chunks_under_limit() {
        let long = "a".repeat(4_200);
        let chunks = split_discord_message(&long);

        assert_eq!(chunks.len(), 3);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.len() <= DISCORD_MESSAGE_LIMIT)
        );
    }

    #[test]
    fn parse_u64_list_accepts_common_separators() {
        assert_eq!(parse_u64_list("1, 2\n3\t4 inválido"), vec![1, 2, 3, 4]);
    }
}
