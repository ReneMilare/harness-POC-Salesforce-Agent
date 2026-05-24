use crate::{
    Config,
    agent::{
        guardrails::{SYSTEM_PROMPT, check_output},
        llm::{self, LlmMessage},
        rag,
        sessions::SessionMessage,
    },
};

pub async fn run(
    config: &Config,
    session_messages: &[SessionMessage],
    user_message: &str,
) -> String {
    let rag_chunks = rag::search(&config.docs_path, user_message, config.rag_top_k);
    let rag_context = rag::format_context(&rag_chunks);

    let mut messages = vec![LlmMessage {
        role: "system".to_string(),
        content: format!(
            "{SYSTEM_PROMPT}\n\nContexto recuperado dos documentos locais:\n{rag_context}\n\n\
            Se o contexto local não for suficiente, diga objetivamente que não encontrou essa informação nos documentos disponíveis."
        ),
    }];
    messages.extend(llm::session_to_messages(session_messages));
    messages.push(LlmMessage {
        role: "user".to_string(),
        content: user_message.to_string(),
    });

    let response = match llm::complete(config, messages).await {
        Ok(response) => response,
        Err(error) => fallback_response(user_message, &rag_context, &error.to_string()),
    };

    let (_, safe_response) = check_output(&response);
    safe_response
}

fn fallback_response(user_message: &str, rag_context: &str, error: &str) -> String {
    if rag_context.starts_with("Nenhuma informação relevante") {
        return format!(
            "Recebi sua mensagem: \"{user_message}\".\n\n\
            O LLM externo ainda não respondeu ({error}) e não encontrei contexto relevante nos documentos locais."
        );
    }

    format!(
        "O LLM externo ainda não respondeu ({error}). Encontrei este contexto local que pode ajudar:\n\n{rag_context}"
    )
}
