use std::fs;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct ContextPolicy {
    pub allowed_contexts: Vec<String>,
    pub blocked_salesforce_objects: Vec<SalesforceObjectRule>,
    pub messages: ContextMessages,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SalesforceObjectRule {
    pub api_name: String,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ContextMessages {
    pub blocked_context: String,
    pub blocked_salesforce_object: String,
}

impl ContextPolicy {
    pub fn load(path: &str) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn allows(&self, context: &str) -> bool {
        self.allowed_contexts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(context))
    }

    pub fn blocks_salesforce_object(&self, message: &str) -> bool {
        let normalized = normalize(message);
        self.blocked_salesforce_objects.iter().any(|rule| {
            contains_word(&normalized, &normalize(&rule.api_name))
                || rule
                    .aliases
                    .iter()
                    .any(|alias| contains_word(&normalized, &normalize(alias)))
        })
    }
}

impl Default for ContextPolicy {
    fn default() -> Self {
        Self {
            allowed_contexts: vec!["account".to_string(), "local_docs".to_string()],
            blocked_salesforce_objects: vec![
                SalesforceObjectRule {
                    api_name: "Contact".to_string(),
                    aliases: vec![
                        "contact".to_string(),
                        "contacts".to_string(),
                        "contato".to_string(),
                        "contatos".to_string(),
                    ],
                },
                SalesforceObjectRule {
                    api_name: "Lead".to_string(),
                    aliases: vec!["lead".to_string(), "leads".to_string()],
                },
                SalesforceObjectRule {
                    api_name: "Opportunity".to_string(),
                    aliases: vec![
                        "opportunity".to_string(),
                        "opportunities".to_string(),
                        "oportunidade".to_string(),
                        "oportunidades".to_string(),
                    ],
                },
            ],
            messages: ContextMessages {
                blocked_context: "Não posso responder essa pergunta. Esta versão consulta somente informações do objeto Account e dos documentos locais.".to_string(),
                blocked_salesforce_object: "Não posso retornar dados desse objeto Salesforce. Esta versão consulta somente dados do objeto Account e dos documentos locais.".to_string(),
            },
        }
    }
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .replace(['á', 'à', 'ã', 'â'], "a")
        .replace(['é', 'ê'], "e")
        .replace(['í'], "i")
        .replace(['ó', 'ô', 'õ'], "o")
        .replace(['ú'], "u")
        .replace('ç', "c")
}

fn contains_word(text: &str, expected: &str) -> bool {
    text.split(|character: char| !character.is_alphanumeric())
        .any(|word| word == expected)
}
