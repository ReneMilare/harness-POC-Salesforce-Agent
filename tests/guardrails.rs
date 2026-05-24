use vps_rust::agent::guardrails::{COMPETITORS, SYSTEM_PROMPT, check_output};

#[test]
fn normal_response_passes() {
    let (ok, text) = check_output("O processo de onboarding está descrito no manual interno.");

    assert!(ok);
    assert!(text.contains("manual interno"));
}

#[test]
fn empty_response_passes() {
    let (ok, text) = check_output("");

    assert!(ok);
    assert!(text.is_empty());
}

#[test]
fn system_prompt_exists() {
    assert!(SYSTEM_PROMPT.len() > 100);
    assert!(SYSTEM_PROMPT.to_lowercase().contains("concorrente"));
}

#[test]
fn competitors_list_is_not_empty() {
    assert!(!COMPETITORS.is_empty());
}

#[test]
fn competitor_mention_is_blocked() {
    for competitor in COMPETITORS {
        let (ok, safe) = check_output(&format!("O produto da {competitor} é melhor porque..."));

        assert!(!ok, "deveria bloquear menção a '{competitor}'");
        assert!(!safe.to_lowercase().contains(&competitor.to_lowercase()));
    }
}

#[test]
fn competitor_mention_is_case_insensitive() {
    let competitor = COMPETITORS[0].to_uppercase();
    let (ok, _) = check_output(&format!("A empresa {competitor} tem solução similar."));

    assert!(!ok);
}

#[test]
fn safe_message_is_returned_when_blocked() {
    let (_, safe) = check_output(&format!("Recomendo usar {}.", COMPETITORS[0]));

    assert!(!safe.is_empty());
    assert!(safe.to_lowercase().contains("concorrente"));
}
