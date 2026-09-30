use serde::Serialize;

use crate::commands::CommandResult;
use crate::domain::{derive_status, Activity, SessionStatus};
use crate::ports::SessionStore;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub duration_ms: Option<u64>,
    pub status: SessionStatus,
    pub url: Option<String>,
    pub error: Option<String>,
}

pub fn get_session<S: SessionStore>(
    store: &S,
    id: &str,
    activity: Option<Activity>,
) -> CommandResult<SessionSummary> {
    let meta = store.meta(id)?;
    let facts = store.facts(id)?;
    Ok(SessionSummary {
        id: meta.id,
        title: meta.title,
        started_at: meta.started_at,
        duration_ms: meta.duration_ms,
        status: derive_status(&facts, activity),
        url: facts.publish.and_then(|p| p.url),
        error: facts.error,
    })
}

// ponytail: lê cada pasta a cada chamada; cache só se a lista passar de centenas de sessões
pub fn list_sessions<S: SessionStore>(
    store: &S,
    activity: &dyn Fn(&str) -> Option<Activity>,
) -> CommandResult<Vec<SessionSummary>> {
    let ids = store.list_ids()?;
    let mut out = ids
        .iter()
        .map(|id| match get_session(store, id, activity(id)) {
            Ok(summary) => summary,
            Err(e) => SessionSummary {
                id: id.clone(),
                title: id.clone(),
                started_at: String::new(),
                duration_ms: None,
                status: SessionStatus::Error,
                url: None,
                error: Some(e.to_string()),
            },
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(out)
}
