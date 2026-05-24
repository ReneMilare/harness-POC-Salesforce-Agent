use std::{
    collections::HashMap,
    env,
    sync::RwLock,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug)]
struct Session {
    history: Vec<SessionMessage>,
    expires_at: Instant,
}

#[derive(Debug)]
pub struct SessionStore {
    ttl: Duration,
    sessions: RwLock<HashMap<String, Session>>,
}

impl SessionStore {
    pub fn new() -> Self {
        let ttl_seconds = env::var("SESSION_TTL_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3600);

        Self::with_ttl(Duration::from_secs(ttl_seconds))
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_history(&self, session_id: &str) -> Vec<SessionMessage> {
        let mut sessions = self.sessions.write().expect("session lock poisoned");
        let now = Instant::now();

        match sessions.get_mut(session_id) {
            Some(session) if now <= session.expires_at => {
                session.expires_at = now + self.ttl;
                session.history.clone()
            }
            Some(_) => {
                sessions.remove(session_id);
                Vec::new()
            }
            None => Vec::new(),
        }
    }

    pub fn append_message(&self, session_id: &str, role: &str, content: &str) {
        let mut sessions = self.sessions.write().expect("session lock poisoned");
        let now = Instant::now();
        let session = sessions
            .entry(session_id.to_string())
            .and_modify(|session| {
                if now > session.expires_at {
                    session.history.clear();
                }
                session.expires_at = now + self.ttl;
            })
            .or_insert_with(|| Session {
                history: Vec::new(),
                expires_at: now + self.ttl,
            });

        session.history.push(SessionMessage {
            role: role.to_string(),
            content: content.to_string(),
        });
    }

    pub fn clear_session(&self, session_id: &str) {
        self.sessions
            .write()
            .expect("session lock poisoned")
            .remove(session_id);
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}
