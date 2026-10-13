//! Exclusão de uma gravação, com ou sem o documento no Outline (spec 2026-10-08 redesign §5).
use super::CommandResult;
use crate::ports::{SessionStore, Wiki};

/// Com `wiki`, apaga primeiro o documento; se o Outline falhar, nada local é apagado.
pub fn delete_session<S: SessionStore, W: Wiki>(
    store: &S,
    wiki: Option<&W>,
    id: &str,
) -> CommandResult<()> {
    if let Some(w) = wiki {
        if let Some(oid) = store.publish_state(id)?.and_then(|p| p.outline_id) {
            w.delete(&oid)?;
        }
    }
    Ok(store.delete(id)?)
}
