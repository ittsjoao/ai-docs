//! Imagens do rascunho escolhidas pelo operador, sem IA (spec 2026-10-08 §3).
use super::error::finish;
use super::publish::publish_draft;
use super::{CommandError, CommandResult};
use crate::domain::{Manual, PublishState};
use crate::ports::{SessionStore, Wiki};

pub const MAX_IMAGEM: usize = 20 * 1024 * 1024;

fn manual<S: SessionStore>(store: &S, id: &str) -> CommandResult<Manual> {
    store.manual(id)?.ok_or(CommandError::NoSteps)
}

/// Caminho relativo do arquivo de uma imagem válida da sessão: recorte de candidato ou extra.
pub fn image_path<S: SessionStore>(store: &S, id: &str, imagem: &str) -> CommandResult<String> {
    if manual(store, id)?.extras.iter().any(|e| e == imagem) {
        return Ok(format!("crops/{imagem}.png"));
    }
    store
        .candidates(id)?
        .into_iter()
        .find(|c| c.id == imagem)
        .and_then(|c| c.crop)
        .ok_or(CommandError::InvalidState("imagem inexistente nesta sessão"))
}

/// `passo` conta de 1, atravessando as seções, como o "Passo n." do manual.
pub fn set_step_image<S: SessionStore>(
    store: &S,
    id: &str,
    passo: usize,
    imagem: Option<&str>,
) -> CommandResult<()> {
    if let Some(img) = imagem {
        image_path(store, id, img)?;
    }
    let mut m = manual(store, id)?;
    let p = m
        .secoes
        .iter_mut()
        .flat_map(|s| s.passos.iter_mut())
        .nth(passo.checked_sub(1).ok_or(CommandError::InvalidState("passo inexistente"))?)
        .ok_or(CommandError::InvalidState("passo inexistente"))?;
    p.imagem = imagem.map(str::to_string);
    Ok(store.save_manual(id, &m)?)
}

pub fn add_operator_image<S: SessionStore>(
    store: &S,
    id: &str,
    bytes: &[u8],
) -> CommandResult<String> {
    if bytes.len() > MAX_IMAGEM {
        return Err(CommandError::InvalidState("a imagem passa de 20 MB"));
    }
    let mut m = manual(store, id)?;
    let img = store.add_image(id, bytes)?;
    m.extras.push(img.clone());
    store.save_manual(id, &m)?;
    Ok(img)
}

/// "Publicar alterações": render + publish direto; `overwrite` aceita a revisão remota (D9).
pub fn republish<S: SessionStore, W: Wiki>(
    store: &S,
    wiki: &W,
    id: &str,
    overwrite: bool,
) -> CommandResult<PublishState> {
    let result = (|| -> CommandResult<PublishState> {
        if overwrite {
            let mut st = store
                .publish_state(id)?
                .ok_or(CommandError::InvalidState("rascunho não existe"))?;
            if let Some(oid) = st.outline_id.clone() {
                st.revision = Some(wiki.info(&oid)?.revision);
                store.save_publish_state(id, &st)?;
            }
        }
        publish_draft(store, wiki, id)
    })();
    finish(store, id, result)
}
