use std::{thread, time::Duration};

use vps_rust::agent::sessions::SessionStore;

#[test]
fn session_store_expires_history_after_ttl() {
    let store = SessionStore::with_ttl(Duration::from_millis(10));

    store.append_message("s1", "user", "oi");
    thread::sleep(Duration::from_millis(20));

    assert!(store.get_history("s1").is_empty());
}

#[test]
fn session_store_clear_removes_history() {
    let store = SessionStore::with_ttl(Duration::from_secs(60));

    store.append_message("s1", "user", "oi");
    store.clear_session("s1");

    assert!(store.get_history("s1").is_empty());
}
