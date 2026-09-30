use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use super::error::finish;
use super::{CommandError, CommandResult};
use crate::domain::{PublishState, PublishStatus};
use crate::pipeline::outline_md::{extract_attachment_ids, rewrite_images, sha256_hex, strip_frontmatter};
use crate::pipeline::render::render;
use crate::ports::{SessionStore, Wiki};

/// Render + upload das imagens novas + create/update do rascunho. Chamado pelo CLI da skill.
pub fn publish_draft<S: SessionStore, W: Wiki>(store: &S, wiki: &W, id: &str) -> CommandResult<PublishState> {
    let manual = store.manual(id)?.ok_or(CommandError::NoSteps)?;
    let candidates = store.candidates(id)?;
    let mut state = store.publish_state(id)?.ok_or(CommandError::NoCollection)?;
    let rendered = render(&manual, &candidates, id)?;
    store.save_rendered(id, &rendered)?;

    if let (Some(oid), Some(local)) = (state.outline_id.as_deref(), state.revision) {
        let remote = wiki.info(oid)?.revision;
        if remote != local {
            return Err(CommandError::EditedManually { local, remote });
        }
    }

    let mut links = BTreeMap::new();
    for img in &rendered.images {
        let bytes = store.read_file(id, &img.to)?;
        let sha = sha256_hex(&bytes);
        let attachment = match state.images.get(&sha) {
            Some(a) => a.clone(),
            None => {
                let name = img.to.rsplit('/').next().unwrap_or(&img.to);
                let a = wiki.upload_image(state.outline_id.as_deref(), name, &bytes)?;
                state.images.insert(sha, a.clone());
                a
            }
        };
        links.insert(img.to.clone(), attachment);
    }

    let text = rewrite_images(strip_frontmatter(&rendered.markdown), &links);
    let doc = match state.outline_id.as_deref() {
        None => wiki.create_draft(&state.collection_id, manual.titulo.trim(), &text)?,
        Some(oid) => wiki.update(oid, manual.titulo.trim(), &text)?,
    };
    state.outline_id = Some(doc.id);
    state.url = Some(doc.url);
    state.revision = Some(doc.revision);
    state.status.get_or_insert(PublishStatus::Draft);
    store.save_publish_state(id, &state)?;
    Ok(state)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FetchReport {
    /// anexos referenciados no doc que não puderam ser baixados
    pub missing: Vec<String>,
    /// anexos cujo conteúdo não bate com o sha256 que o app enviou
    pub mismatched: Vec<String>,
}

pub fn fetch_published<S: SessionStore, W: Wiki>(store: &S, wiki: &W, id: &str) -> CommandResult<FetchReport> {
    let state = store.publish_state(id)?.ok_or(CommandError::InvalidState("manual ainda não publicado"))?;
    let oid = state.outline_id.as_deref().ok_or(CommandError::InvalidState("manual ainda não publicado"))?;
    let doc = wiki.info(oid)?;
    let sha_by_attachment: HashMap<&str, &str> = state.images.iter().map(|(sha, a)| (a.as_str(), sha.as_str())).collect();
    let mut report = FetchReport::default();
    let mut images = Vec::new();
    for attachment in extract_attachment_ids(&doc.text) {
        match wiki.download_attachment(&attachment) {
            Ok(bytes) => {
                if sha_by_attachment.get(attachment.as_str()) != Some(&sha256_hex(&bytes).as_str()) {
                    report.mismatched.push(attachment.clone());
                }
                images.push((attachment, bytes));
            }
            Err(_) => report.missing.push(attachment),
        }
    }
    store.save_published(id, &doc.text, &images)?;
    Ok(report)
}

pub fn approve<S: SessionStore, W: Wiki>(store: &S, wiki: &W, id: &str) -> CommandResult<PublishState> {
    let result = (|| -> CommandResult<PublishState> {
        let mut state = store.publish_state(id)?.ok_or(CommandError::InvalidState("rascunho não existe"))?;
        let oid = state.outline_id.clone().ok_or(CommandError::InvalidState("rascunho não existe"))?;
        if state.status == Some(PublishStatus::Published) {
            return Err(CommandError::InvalidState("manual já publicado"));
        }
        let doc = wiki.publish(&oid)?;
        state.status = Some(PublishStatus::Published);
        state.revision = Some(doc.revision);
        state.url = Some(doc.url);
        store.save_publish_state(id, &state)?;
        Ok(state)
    })();
    finish(store, id, result)
}
