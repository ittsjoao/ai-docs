use super::error::finish;
use super::{CommandError, CommandResult};
use crate::domain::PublishState;
use crate::ports::{AgentMode, AgentResult, ManualAgent, SessionStore, Wiki};

pub fn generate_manual<S: SessionStore, A: ManualAgent>(
    store: &S,
    agent: &A,
    id: &str,
    collection_id: &str,
    progress: &mut dyn FnMut(&str),
) -> CommandResult<AgentResult> {
    if !store.facts(id)?.has_candidates {
        return Err(CommandError::InvalidState("processe a gravação antes de gerar o manual"));
    }
    let result = (|| -> CommandResult<AgentResult> {
        let mut state = store.publish_state(id)?.unwrap_or_else(|| PublishState::new(collection_id));
        state.collection_id = collection_id.to_string();
        store.save_publish_state(id, &state)?;
        Ok(agent.run(&store.dir(id), AgentMode::Gerar, progress)?)
    })();
    finish(store, id, result)
}

pub fn improve_manual<S: SessionStore, W: Wiki, A: ManualAgent>(
    store: &S,
    wiki: &W,
    agent: &A,
    id: &str,
    text: &str,
    overwrite: bool,
    progress: &mut dyn FnMut(&str),
) -> CommandResult<AgentResult> {
    let text = text.trim();
    if text.is_empty() {
        return Err(CommandError::InvalidState("descreva a melhoria desejada"));
    }
    let mut state = store.publish_state(id)?.ok_or(CommandError::InvalidState("gere o manual antes de pedir melhoria"))?;
    let oid = state.outline_id.clone().ok_or(CommandError::InvalidState("gere o manual antes de pedir melhoria"))?;
    let remote = wiki.info(&oid)?.revision;
    if state.revision != Some(remote) {
        if !overwrite {
            return Err(CommandError::EditedManually { local: state.revision.unwrap_or(0), remote });
        }
        state.revision = Some(remote);
        store.save_publish_state(id, &state)?;
    }
    store.append_feedback(id, text)?;
    let result = agent.run(&store.dir(id), AgentMode::Melhoria, progress).map_err(CommandError::from);
    finish(store, id, result)
}
