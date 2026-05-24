use std::collections::{HashMap, HashSet};

use crate::{
    Config,
    agent::{
        llm::{self, LlmMessage},
        rag,
        sessions::SessionMessage,
    },
    api::models::{
        AccountFieldDescriptor, AccountQueryFilter, AccountQueryIntent, AccountQueryResult,
    },
    context_policy::ContextPolicy,
};

const ACCOUNT_OBJECT: &str = "Account";
const ACCOUNT_OPERATION: &str = "get_or_list";
const MAX_ACCOUNT_ROWS: u8 = 5;

#[derive(Debug)]
pub enum PlanOutcome {
    AccountIntent(AccountQueryIntent),
    DirectResponse(String),
    NormalResponse(String),
}

pub async fn plan(
    config: &Config,
    history: &[SessionMessage],
    message: &str,
    fields: &[AccountFieldDescriptor],
) -> PlanOutcome {
    let policy = ContextPolicy::load(&config.context_policy_path);

    if policy.blocks_salesforce_object(message) {
        return PlanOutcome::DirectResponse(policy.messages.blocked_salesforce_object);
    }

    if !looks_like_account_question(message, fields) {
        if policy.allows("local_docs") && has_local_document_context(config, message) {
            return PlanOutcome::NormalResponse(
                "Consulta direcionada aos documentos locais.".to_string(),
            );
        }

        return PlanOutcome::DirectResponse(policy.messages.blocked_context);
    }

    if !policy.allows("account") {
        return PlanOutcome::DirectResponse(policy.messages.blocked_context);
    }

    if let Some(intent) = llm_account_intent(config, history, message, fields).await {
        return PlanOutcome::AccountIntent(intent);
    }

    PlanOutcome::AccountIntent(heuristic_account_intent(message, fields))
}

pub fn should_handle_salesforce_message(
    config: &Config,
    message: &str,
    fields: &[AccountFieldDescriptor],
) -> bool {
    let policy = ContextPolicy::load(&config.context_policy_path);
    policy.blocks_salesforce_object(message) || looks_like_account_question(message, fields)
}

pub async fn finalize(
    config: &Config,
    message: &str,
    intent: &AccountQueryIntent,
    result: &AccountQueryResult,
) -> String {
    if let Ok(response) = llm_account_response(config, message, intent, result).await {
        return response;
    }

    fallback_account_response(result)
}

pub fn sanitize_intent(
    intent: AccountQueryIntent,
    fields: &[AccountFieldDescriptor],
) -> AccountQueryIntent {
    let allowed = allowed_field_names(fields);
    let requested_fields = sanitize_requested_fields(intent.requested_fields, &allowed);
    let filters = intent
        .filters
        .into_iter()
        .filter(|filter| allowed.contains(&filter.field))
        .filter(|filter| is_allowed_operator(&filter.operator))
        .collect();

    AccountQueryIntent {
        object: ACCOUNT_OBJECT.to_string(),
        operation: ACCOUNT_OPERATION.to_string(),
        requested_fields,
        filters,
        limit: intent.limit.clamp(1, MAX_ACCOUNT_ROWS),
    }
}

fn looks_like_account_question(message: &str, fields: &[AccountFieldDescriptor]) -> bool {
    let normalized = normalize(message);
    contains_word(&normalized, "conta")
        || contains_word(&normalized, "account")
        || fields.iter().any(|field| {
            normalized.contains(&normalize(&field.api_name))
                || normalized.contains(&normalize(&field.label))
        })
}

fn has_local_document_context(config: &Config, message: &str) -> bool {
    rag::search(&config.docs_path, message, 1)
        .first()
        .map(|chunk| chunk.score > 0)
        .unwrap_or(false)
}

async fn llm_account_intent(
    config: &Config,
    history: &[SessionMessage],
    message: &str,
    fields: &[AccountFieldDescriptor],
) -> Option<AccountQueryIntent> {
    let catalog = serde_json::to_string(fields).ok()?;
    let prompt = format!(
        "Você transforma mensagens em JSON estrito para consultar somente Account.\n\
        Use apenas campos deste catálogo legível pelo usuário: {catalog}\n\
        Regras: object sempre Account; operation sempre get_or_list; limit máximo 5; \
        não use relacionamentos, subqueries, agregações nem SOQL livre.\n\
        Responda só JSON no formato: \
        {{\"object\":\"Account\",\"operation\":\"get_or_list\",\"requested_fields\":[\"Id\",\"Name\"],\
        \"filters\":[{{\"field\":\"Name\",\"operator\":\"contains\",\"value\":\"Acme\"}}],\"limit\":5}}."
    );
    let mut messages = vec![LlmMessage {
        role: "system".to_string(),
        content: prompt,
    }];
    messages.extend(llm::session_to_messages(history));
    messages.push(LlmMessage {
        role: "user".to_string(),
        content: message.to_string(),
    });

    let response = llm::complete(config, messages).await.ok()?;
    let json = extract_json_object(&response)?;
    let intent = serde_json::from_str::<AccountQueryIntent>(json).ok()?;
    Some(sanitize_intent(intent, fields))
}

async fn llm_account_response(
    config: &Config,
    message: &str,
    intent: &AccountQueryIntent,
    result: &AccountQueryResult,
) -> Result<String, llm::LlmError> {
    let intent_json = serde_json::to_string(intent).unwrap_or_default();
    let result_json = serde_json::to_string(result).unwrap_or_default();
    let messages = vec![
        LlmMessage {
            role: "system".to_string(),
            content: account_response_prompt(),
        },
        LlmMessage {
            role: "user".to_string(),
            content: format!(
                "Pergunta original: {message}\nIntent: {intent_json}\nResultado Account: {result_json}"
            ),
        },
    ];

    llm::complete(config, messages).await
}

fn heuristic_account_intent(
    message: &str,
    fields: &[AccountFieldDescriptor],
) -> AccountQueryIntent {
    let allowed = allowed_field_names(fields);
    let requested_fields = requested_fields_from_text(message, fields, &allowed);
    let filters = name_filter_from_text(message)
        .map(|value| AccountQueryFilter {
            field: "Name".to_string(),
            operator: "contains".to_string(),
            value,
        })
        .into_iter()
        .collect();

    AccountQueryIntent {
        object: ACCOUNT_OBJECT.to_string(),
        operation: ACCOUNT_OPERATION.to_string(),
        requested_fields,
        filters,
        limit: MAX_ACCOUNT_ROWS,
    }
}

fn requested_fields_from_text(
    message: &str,
    fields: &[AccountFieldDescriptor],
    allowed: &HashSet<String>,
) -> Vec<String> {
    let normalized = normalize(message);
    let aliases = HashMap::from([
        ("telefone", "Phone"),
        ("phone", "Phone"),
        ("site", "Website"),
        ("website", "Website"),
        ("industria", "Industry"),
        ("indústria", "Industry"),
        ("tipo", "Type"),
        ("cidade", "BillingCity"),
        ("estado", "BillingState"),
        ("endereco", "BillingStreet"),
        ("endereço", "BillingStreet"),
    ]);
    let mut selected = vec!["Id".to_string(), "Name".to_string()];

    for field in fields {
        if normalized.contains(&normalize(&field.api_name))
            || normalized.contains(&normalize(&field.label))
        {
            selected.push(field.api_name.clone());
        }
    }

    for (alias, api_name) in aliases {
        if normalized.contains(alias) && allowed.contains(api_name) {
            selected.push(api_name.to_string());
        }
    }

    sanitize_requested_fields(selected, allowed)
}

fn sanitize_requested_fields(
    requested_fields: Vec<String>,
    allowed: &HashSet<String>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut sanitized = Vec::new();

    for field in requested_fields {
        if allowed.contains(&field) && seen.insert(field.clone()) {
            sanitized.push(field);
        }
    }

    for required in ["Id", "Name"] {
        if allowed.contains(required) && !seen.contains(required) {
            sanitized.insert(0, required.to_string());
        }
    }

    sanitized
}

fn name_filter_from_text(message: &str) -> Option<String> {
    let normalized = message
        .replace('?', " ")
        .replace('.', " ")
        .replace(',', " ");
    let lower = normalized.to_lowercase();
    let marker = lower.find("conta ").or_else(|| lower.find("account "))?;
    let raw = normalized[marker..]
        .split_whitespace()
        .skip(1)
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    let value = raw.trim().to_string();

    if value.is_empty() { None } else { Some(value) }
}

fn allowed_field_names(fields: &[AccountFieldDescriptor]) -> HashSet<String> {
    fields.iter().map(|field| field.api_name.clone()).collect()
}

fn is_allowed_operator(operator: &str) -> bool {
    matches!(operator, "equals" | "contains")
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    text.get(start..=end)
}

fn fallback_account_response(result: &AccountQueryResult) -> String {
    if !result.errors.is_empty() {
        return format!(
            "Não foi possível consultar Account: {}",
            result.errors.join("; ")
        );
    }

    if result.records.is_empty() {
        return "Não encontrei contas com os critérios informados.".to_string();
    }

    let mut lines = result
        .records
        .iter()
        .enumerate()
        .map(|(index, record)| format_record(index + 1, record))
        .collect::<Vec<_>>();

    if result.has_more {
        lines.push("Há mais resultados. Informe um filtro mais específico.".to_string());
    }

    lines.join("\n")
}

fn account_response_prompt() -> String {
    "Responda em pt-BR, de forma objetiva e bonita para o usuário final. \
    Use somente os dados de Account fornecidos. Não use Markdown com asteriscos, \
    não escreva rótulos como **Nome:**, não cite campos sem valor e não invente dados. \
    Se houver uma única conta, use este estilo: primeira linha com o nome da conta; \
    depois seções curtas como Resumo, Contato, Endereço e Identificação, somente quando houver dados. \
    Se houver várias contas, liste no máximo os registros recebidos e peça desambiguação. \
    Se houver erro, explique sem expor detalhes internos."
        .to_string()
}

fn format_record(index: usize, record: &HashMap<String, serde_json::Value>) -> String {
    let title = record
        .get("Name")
        .map(format_value)
        .filter(|value| value != "sem valor")
        .unwrap_or_else(|| format!("Conta {index}"));
    let mut pairs = record
        .iter()
        .filter(|(key, _)| key.as_str() != "Name")
        .map(|(key, value)| format!("{key}: {}", format_value(value)))
        .collect::<Vec<_>>();
    pairs.sort();

    if pairs.is_empty() {
        return format!("{index}. {title}");
    }

    format!("{index}. {title}\n   {}", pairs.join(" | "))
}

fn format_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "sem valor".to_string(),
        serde_json::Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
}

fn contains_word(text: &str, expected: &str) -> bool {
    text.split(|character: char| !character.is_alphanumeric())
        .any(|word| word == expected)
}
