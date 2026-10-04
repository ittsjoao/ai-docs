use crate::pipeline::render::RenderError;
use crate::ports::SessionStore;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("ação inválida no estado atual: {0}")]
    InvalidState(&'static str),
    #[error("steps.json não encontrado; gere o manual primeiro")]
    NoSteps,
    #[error("coleção do Outline não definida para esta sessão")]
    NoCollection,
    #[error(
        "o documento foi editado no Outline (revisão {remote}; última publicada pelo app: {local})"
    )]
    EditedManually { local: u64, remote: u64 },
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Unknown(#[from] anyhow::Error),
}

pub type CommandResult<T> = Result<T, CommandError>;

/// Registra o resultado na sessão: limpa o erro no sucesso, grava a mensagem na falha.
pub(crate) fn finish<S: SessionStore, T>(
    store: &S,
    id: &str,
    result: CommandResult<T>,
) -> CommandResult<T> {
    let message = result.as_ref().err().map(|e| match e {
        // `{:#}` mostra a cadeia inteira de contextos do anyhow
        CommandError::Unknown(inner) => format!("{inner:#}"),
        other => other.to_string(),
    });
    let recorded = store.set_error(id, message.as_deref());
    match result {
        Ok(v) => recorded.map(|_| v).map_err(Into::into),
        err => err,
    }
}
