use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub session_id: String,
    pub message: String,
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub session_id: String,
    pub response: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AccountFieldDescriptor {
    pub api_name: String,
    pub label: String,
    pub data_type: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatPlanRequest {
    pub session_id: String,
    pub message: String,
    pub user_id: String,
    pub account_fields: Vec<AccountFieldDescriptor>,
}

#[derive(Debug, Serialize)]
pub struct ChatPlanResponse {
    pub session_id: String,
    pub response: Option<String>,
    pub account_intent: Option<AccountQueryIntent>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AccountQueryIntent {
    pub object: String,
    pub operation: String,
    pub requested_fields: Vec<String>,
    pub filters: Vec<AccountQueryFilter>,
    pub limit: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AccountQueryFilter {
    pub field: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatFinalizeRequest {
    pub session_id: String,
    pub message: String,
    pub user_id: String,
    pub account_intent: AccountQueryIntent,
    pub account_result: AccountQueryResult,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AccountQueryResult {
    pub status: String,
    pub records: Vec<HashMap<String, Value>>,
    pub errors: Vec<String>,
    pub has_more: bool,
}
