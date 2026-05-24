use std::{
    collections::{HashMap, HashSet},
    fs,
    sync::{OnceLock, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Config,
    agent::{
        account_intent::{self, PlanOutcome},
        guardrails::check_output,
        sessions::SessionMessage,
    },
    api::models::{
        AccountFieldDescriptor, AccountQueryFilter, AccountQueryIntent, AccountQueryResult,
    },
};

const MAX_ACCOUNT_ROWS: u8 = 5;
const TOKEN_REFRESH_BUFFER: Duration = Duration::from_secs(300);

static TOKEN_CACHE: OnceLock<RwLock<Option<CachedToken>>> = OnceLock::new();

#[derive(Clone, Debug)]
struct CachedToken {
    access_token: String,
    instance_url: String,
    expires_at: SystemTime,
    cache_key: String,
}

#[derive(Debug)]
pub enum SalesforceError {
    MissingConfig,
    PrivateKey(String),
    Jwt(String),
    Request(String),
    Api(u16, String),
    InvalidResponse(String),
}

impl std::fmt::Display for SalesforceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingConfig => write!(formatter, "configuração Salesforce incompleta"),
            Self::PrivateKey(error) => {
                write!(formatter, "falha ao ler chave privada Salesforce: {error}")
            }
            Self::Jwt(error) => write!(formatter, "falha ao assinar JWT Salesforce: {error}"),
            Self::Request(error) => write!(formatter, "falha na chamada Salesforce: {error}"),
            Self::Api(status, body) => {
                write!(formatter, "Salesforce retornou HTTP {status}: {body}")
            }
            Self::InvalidResponse(error) => {
                write!(formatter, "resposta Salesforce inválida: {error}")
            }
        }
    }
}

impl std::error::Error for SalesforceError {}

#[derive(Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    exp: usize,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    instance_url: String,
}

#[derive(Clone, Debug)]
struct AccountFieldInfo {
    descriptor: AccountFieldDescriptor,
    filterable: bool,
}

#[derive(Deserialize)]
struct DescribeResponse {
    fields: Vec<DescribeField>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DescribeField {
    name: String,
    label: String,
    #[serde(rename = "type")]
    data_type: String,
    filterable: bool,
    #[serde(default)]
    deprecated_and_hidden: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryResponse {
    total_size: usize,
    records: Vec<HashMap<String, Value>>,
}

pub async fn answer_account_question(
    config: &Config,
    history: &[SessionMessage],
    message: &str,
) -> Option<String> {
    if !is_configured(config) {
        return None;
    }

    let fields = match describe_account_fields(config).await {
        Ok(fields) => fields,
        Err(error) => {
            tracing::warn!(%error, "falha ao carregar catálogo Account do Salesforce");
            return message_mentions_salesforce(message).then(|| {
                "Não consegui consultar o Salesforce agora. Verifique a configuração do usuário técnico."
                    .to_string()
            });
        }
    };
    let descriptors = fields
        .iter()
        .map(|field| field.descriptor.clone())
        .collect::<Vec<_>>();

    if !should_try_account_flow(config, message, &descriptors) {
        return None;
    }

    match account_intent::plan(config, history, message, &descriptors).await {
        PlanOutcome::AccountIntent(intent) => {
            let result = execute_account_query(config, intent, &fields).await;
            let response =
                account_intent::finalize(config, message, &result.intent, &result.result).await;
            let (_, safe_response) = check_output(&response);
            Some(safe_response)
        }
        PlanOutcome::DirectResponse(response) => Some(response),
        PlanOutcome::NormalResponse(_) => None,
    }
}

struct QueryExecution {
    intent: AccountQueryIntent,
    result: AccountQueryResult,
}

async fn execute_account_query(
    config: &Config,
    intent: AccountQueryIntent,
    fields: &[AccountFieldInfo],
) -> QueryExecution {
    let descriptors = fields
        .iter()
        .map(|field| field.descriptor.clone())
        .collect::<Vec<_>>();
    let sanitized = account_intent::sanitize_intent(intent, &descriptors);
    let result = match query_accounts(config, &sanitized, fields).await {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(%error, "falha ao executar consulta Account no Salesforce");
            AccountQueryResult {
                status: "invalid".to_string(),
                records: Vec::new(),
                errors: vec!["Consulta Account indisponível para o acesso atual.".to_string()],
                has_more: false,
            }
        }
    };

    QueryExecution {
        intent: sanitized,
        result,
    }
}

async fn describe_account_fields(
    config: &Config,
) -> Result<Vec<AccountFieldInfo>, SalesforceError> {
    let value = sf_get(config, "/sobjects/Account/describe", &[]).await?;
    let describe = serde_json::from_value::<DescribeResponse>(value)
        .map_err(|error| SalesforceError::InvalidResponse(error.to_string()))?;
    let mut fields = describe
        .fields
        .into_iter()
        .filter(|field| !field.deprecated_and_hidden)
        .map(|field| AccountFieldInfo {
            descriptor: AccountFieldDescriptor {
                api_name: field.name,
                label: field.label,
                data_type: field.data_type,
            },
            filterable: field.filterable,
        })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.descriptor.api_name.cmp(&right.descriptor.api_name));
    Ok(fields)
}

async fn query_accounts(
    config: &Config,
    intent: &AccountQueryIntent,
    fields: &[AccountFieldInfo],
) -> Result<AccountQueryResult, SalesforceError> {
    let field_map = fields
        .iter()
        .map(|field| (field.descriptor.api_name.as_str(), field))
        .collect::<HashMap<_, _>>();
    let mut errors = Vec::new();
    let requested_fields = validated_requested_fields(intent, &field_map, &mut errors);
    let where_clause = validated_where_clause(&intent.filters, &field_map, &mut errors);

    if !errors.is_empty() {
        return Ok(AccountQueryResult {
            status: "invalid".to_string(),
            records: Vec::new(),
            errors,
            has_more: false,
        });
    }

    let query_limit = usize::from(intent.limit.clamp(1, MAX_ACCOUNT_ROWS)) + 1;
    let soql = build_account_soql(&requested_fields, &where_clause, query_limit);
    let value = sf_get(config, "/query", &[("q", soql.as_str())]).await?;
    let mut response = serde_json::from_value::<QueryResponse>(value)
        .map_err(|error| SalesforceError::InvalidResponse(error.to_string()))?;

    let has_more = response.total_size > usize::from(MAX_ACCOUNT_ROWS)
        || response.records.len() > usize::from(MAX_ACCOUNT_ROWS);
    response.records.truncate(usize::from(MAX_ACCOUNT_ROWS));
    for record in &mut response.records {
        record.remove("attributes");
    }

    Ok(AccountQueryResult {
        status: "ok".to_string(),
        records: response.records,
        errors: Vec::new(),
        has_more,
    })
}

fn validated_requested_fields(
    intent: &AccountQueryIntent,
    field_map: &HashMap<&str, &AccountFieldInfo>,
    errors: &mut Vec<String>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut requested = Vec::new();

    for field_name in ["Id", "Name"]
        .into_iter()
        .chain(intent.requested_fields.iter().map(String::as_str))
    {
        if field_name.contains('.') || !field_map.contains_key(field_name) {
            errors.push(format!("Campo indisponível para Account: {field_name}"));
            continue;
        }
        if seen.insert(field_name.to_string()) {
            requested.push(field_name.to_string());
        }
    }

    requested
}

fn validated_where_clause(
    filters: &[AccountQueryFilter],
    field_map: &HashMap<&str, &AccountFieldInfo>,
    errors: &mut Vec<String>,
) -> Vec<String> {
    filters
        .iter()
        .filter_map(|filter| {
            let Some(field) = field_map.get(filter.field.as_str()) else {
                errors.push(format!(
                    "Filtro indisponível para Account: {}",
                    filter.field
                ));
                return None;
            };
            if filter.field.contains('.') || !field.filterable {
                errors.push(format!(
                    "Filtro indisponível para Account: {}",
                    filter.field
                ));
                return None;
            }

            let value = soql_string_literal(&filter.value);
            match filter.operator.as_str() {
                "equals" => Some(format!("{} = {value}", filter.field)),
                "contains" => Some(format!(
                    "{} LIKE {}",
                    filter.field,
                    soql_like_literal(&filter.value)
                )),
                operator => {
                    errors.push(format!("Operador de filtro não permitido: {operator}"));
                    None
                }
            }
        })
        .collect()
}

fn build_account_soql(fields: &[String], where_clause: &[String], limit: usize) -> String {
    let mut soql = format!("SELECT {} FROM Account", fields.join(","));
    if !where_clause.is_empty() {
        soql.push_str(" WHERE ");
        soql.push_str(&where_clause.join(" AND "));
    }
    soql.push_str(" ORDER BY Name ASC NULLS LAST LIMIT ");
    soql.push_str(&limit.to_string());
    soql
}

fn soql_string_literal(value: &str) -> String {
    format!("'{}'", escape_soql(value))
}

fn soql_like_literal(value: &str) -> String {
    format!("'%{}%'", escape_soql(value))
}

fn escape_soql(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

async fn sf_get(
    config: &Config,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Value, SalesforceError> {
    let token = access_token(config).await?;
    let url = format!(
        "{}/services/data/{}{}",
        token.instance_url.trim_end_matches('/'),
        config.sf_api_version.trim_matches('/'),
        path
    );
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(token.access_token)
        .query(query)
        .send()
        .await
        .map_err(|error| SalesforceError::Request(error.to_string()))?;
    parse_response(response).await
}

async fn access_token(config: &Config) -> Result<CachedToken, SalesforceError> {
    if !is_configured(config) {
        return Err(SalesforceError::MissingConfig);
    }

    let cache_key = format!(
        "{}|{}|{}",
        config.sf_login_url, config.sf_client_id, config.sf_username
    );
    let cache = TOKEN_CACHE.get_or_init(|| RwLock::new(None));
    if let Some(token) = cache
        .read()
        .expect("salesforce token cache lock poisoned")
        .as_ref()
        .filter(|token| token.cache_key == cache_key && token.expires_at > SystemTime::now())
        .cloned()
    {
        return Ok(token);
    }

    let token = request_access_token(config, cache_key).await?;
    *cache.write().expect("salesforce token cache lock poisoned") = Some(token.clone());
    Ok(token)
}

async fn request_access_token(
    config: &Config,
    cache_key: String,
) -> Result<CachedToken, SalesforceError> {
    let private_key = fs::read(&config.sf_private_key_path)
        .map_err(|error| SalesforceError::PrivateKey(error.to_string()))?;
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as usize
        + 300;
    let claims = JwtClaims {
        iss: &config.sf_client_id,
        sub: &config.sf_username,
        aud: &config.sf_login_url,
        exp,
    };
    let assertion = encode(
        &Header::new(Algorithm::RS256),
        &claims,
        &EncodingKey::from_rsa_pem(&private_key)
            .map_err(|error| SalesforceError::Jwt(error.to_string()))?,
    )
    .map_err(|error| SalesforceError::Jwt(error.to_string()))?;

    let token_url = format!(
        "{}/services/oauth2/token",
        config.sf_login_url.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(token_url)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", assertion.as_str()),
        ])
        .send()
        .await
        .map_err(|error| SalesforceError::Request(error.to_string()))?;
    let value = parse_response(response).await?;
    let token = serde_json::from_value::<TokenResponse>(value)
        .map_err(|error| SalesforceError::InvalidResponse(error.to_string()))?;

    Ok(CachedToken {
        access_token: token.access_token,
        instance_url: token.instance_url,
        expires_at: SystemTime::now() + Duration::from_secs(3600) - TOKEN_REFRESH_BUFFER,
        cache_key,
    })
}

async fn parse_response(response: reqwest::Response) -> Result<Value, SalesforceError> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| SalesforceError::Request(error.to_string()))?;

    if !status.is_success() {
        return Err(SalesforceError::Api(status.as_u16(), body));
    }

    serde_json::from_str(&body).map_err(|error| SalesforceError::InvalidResponse(error.to_string()))
}

fn is_configured(config: &Config) -> bool {
    !config.sf_login_url.trim().is_empty()
        && !config.sf_client_id.trim().is_empty()
        && !config.sf_username.trim().is_empty()
        && !config.sf_private_key_path.trim().is_empty()
}

fn should_try_account_flow(
    config: &Config,
    message: &str,
    fields: &[AccountFieldDescriptor],
) -> bool {
    if message_mentions_salesforce(message) {
        return true;
    }

    let normalized = message.to_lowercase();
    let field_match = fields.iter().any(|field| {
        normalized.contains(&field.api_name.to_lowercase())
            || normalized.contains(&field.label.to_lowercase())
    });

    field_match || account_intent::should_handle_salesforce_message(config, message, fields)
}

fn message_mentions_salesforce(message: &str) -> bool {
    let normalized = message.to_lowercase();
    normalized.contains("salesforce")
        || normalized
            .split(|character: char| !character.is_alphanumeric())
            .any(|word| matches!(word, "conta" | "account" | "cliente"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, filterable: bool) -> AccountFieldInfo {
        AccountFieldInfo {
            descriptor: AccountFieldDescriptor {
                api_name: name.to_string(),
                label: name.to_string(),
                data_type: "string".to_string(),
            },
            filterable,
        }
    }

    #[test]
    fn build_account_soql_uses_only_validated_fields_and_limit() {
        let soql = build_account_soql(
            &["Id".to_string(), "Name".to_string(), "Phone".to_string()],
            &["Name LIKE '%Acme%'".to_string()],
            6,
        );

        assert_eq!(
            soql,
            "SELECT Id,Name,Phone FROM Account WHERE Name LIKE '%Acme%' ORDER BY Name ASC NULLS LAST LIMIT 6"
        );
    }

    #[test]
    fn validates_requested_fields_rejects_relationships() {
        let fields = [field("Id", true), field("Name", true)];
        let field_map = fields
            .iter()
            .map(|field| (field.descriptor.api_name.as_str(), field))
            .collect::<HashMap<_, _>>();
        let intent = AccountQueryIntent {
            object: "Account".to_string(),
            operation: "get_or_list".to_string(),
            requested_fields: vec!["Owner.Name".to_string()],
            filters: Vec::new(),
            limit: 5,
        };
        let mut errors = Vec::new();

        let requested = validated_requested_fields(&intent, &field_map, &mut errors);

        assert_eq!(requested, vec!["Id".to_string(), "Name".to_string()]);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn validated_where_clause_rejects_unfilterable_field() {
        let fields = [field("Id", true), field("Name", false)];
        let field_map = fields
            .iter()
            .map(|field| (field.descriptor.api_name.as_str(), field))
            .collect::<HashMap<_, _>>();
        let filters = vec![AccountQueryFilter {
            field: "Name".to_string(),
            operator: "contains".to_string(),
            value: "Acme".to_string(),
        }];
        let mut errors = Vec::new();

        let clauses = validated_where_clause(&filters, &field_map, &mut errors);

        assert!(clauses.is_empty());
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn escape_soql_escapes_quotes_and_backslashes() {
        assert_eq!(escape_soql("A\\B's"), "A\\\\B\\'s");
    }
}
