//! `#[tauri::command]` finos: cada um chama o `App` numa thread de bloqueio (spec 2026-10-05 §3).
use screenmanual_app::{
    validar_id, ApiError, App, Deps, Detalhe, EstadoGravacao, ImagemUi, Inicio, PassoUi,
};
use screenmanual_core::domain::{Perguntas, Respostas, TranscriptionModel};
use screenmanual_core::ports::{AgentOutcome, Collection, DocNode, SessionStore};
use screenmanual_core::queries::SessionSummary;
use screenmanual_settings::AppConfig;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::real::RealDeps;
use crate::AppState;

type St<'a> = State<'a, AppState>;

async fn bloq<T, F>(st: &AppState, f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&App<RealDeps>) -> Result<T, ApiError> + Send + 'static,
{
    let st = st.clone();
    tauri::async_runtime::spawn_blocking(move || f(&st))
        .await
        .map_err(|e| ApiError::new("outro", e.to_string()))?
}

fn falha(e: impl std::fmt::Display) -> ApiError {
    ApiError::new("outro", e.to_string())
}

#[tauri::command]
pub async fn inicio(st: St<'_>) -> Result<Inicio, ApiError> {
    bloq(&st, |a| a.inicio()).await
}

#[tauri::command]
pub async fn sessoes(st: St<'_>) -> Result<Vec<SessionSummary>, ApiError> {
    bloq(&st, |a| a.sessoes()).await
}

#[tauri::command]
pub async fn detalhe(st: St<'_>, id: String) -> Result<Detalhe, ApiError> {
    bloq(&st, move |a| a.detalhe(&id)).await
}

#[tauri::command]
pub async fn config(st: St<'_>) -> Result<AppConfig, ApiError> {
    bloq(&st, |a| a.config()).await
}

#[tauri::command]
pub async fn salvar_config(st: St<'_>, cfg: AppConfig) -> Result<(), ApiError> {
    bloq(&st, move |a| a.salvar_config(&cfg)).await
}

#[tauri::command]
pub async fn conectar_outline(
    st: St<'_>,
    url: String,
    token: String,
) -> Result<Vec<Collection>, ApiError> {
    bloq(&st, move |a| a.conectar_outline(&url, &token)).await
}

#[tauri::command]
pub async fn documentos(st: St<'_>, colecao: String) -> Result<Vec<DocNode>, ApiError> {
    bloq(&st, move |a| a.documentos(&colecao)).await
}

#[tauri::command]
pub async fn colecoes(st: St<'_>) -> Result<Vec<Collection>, ApiError> {
    bloq(&st, |a| a.colecoes()).await
}

#[tauri::command]
pub async fn baixar_modelo(st: St<'_>, modelo: TranscriptionModel) -> Result<(), ApiError> {
    bloq(&st, move |a| a.baixar_modelo(modelo)).await
}

#[tauri::command]
pub async fn gravar(st: St<'_>, titulo: String, quando: String) -> Result<String, ApiError> {
    bloq(&st, move |a| a.gravar(&titulo, &quando)).await
}

#[tauri::command]
pub async fn pausar(st: St<'_>) -> Result<EstadoGravacao, ApiError> {
    bloq(&st, |a| a.pausar()).await
}

#[tauri::command]
pub async fn marcar(st: St<'_>) -> Result<(), ApiError> {
    bloq(&st, |a| a.marcar()).await
}

#[tauri::command]
pub async fn parar(st: St<'_>) -> Result<String, ApiError> {
    bloq(&st, |a| a.parar()).await
}

#[tauri::command]
pub async fn processar(st: St<'_>, id: String, refazer: bool) -> Result<usize, ApiError> {
    bloq(&st, move |a| a.processar(&id, refazer)).await
}

#[tauri::command]
pub async fn gerar(
    st: St<'_>,
    id: String,
    colecao: String,
    pai: Option<String>,
    sobrescrever: bool,
) -> Result<AgentOutcome, ApiError> {
    bloq(&st, move |a| {
        a.gerar(&id, &colecao, pai.as_deref(), sobrescrever)
    })
    .await
}

#[tauri::command]
pub async fn melhorar(
    st: St<'_>,
    id: String,
    texto: String,
    sobrescrever: bool,
) -> Result<AgentOutcome, ApiError> {
    bloq(&st, move |a| a.melhorar(&id, &texto, sobrescrever)).await
}

#[tauri::command]
pub async fn aprovar(st: St<'_>, id: String) -> Result<(), ApiError> {
    bloq(&st, move |a| a.aprovar(&id)).await
}

#[tauri::command]
pub fn cancelar(st: St<'_>) {
    st.cancelar();
}

#[tauri::command]
pub fn abrir_link(app: AppHandle, url: String) -> Result<(), ApiError> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(ApiError::new("estado_invalido", "link inválido"));
    }
    app.opener().open_url(url, None::<&str>).map_err(falha)
}

#[tauri::command]
pub fn abrir_pasta(app: AppHandle, st: St<'_>, id: String) -> Result<(), ApiError> {
    validar_id(&id)?;
    let dir = st.deps().store().dir(&id);
    if !dir.is_dir() {
        return Err(ApiError::new("estado_invalido", "sessão inválida"));
    }
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(falha)
}

/// Abre um console visível com `claude` para o operador fazer login (spec §7.2).
#[tauri::command]
pub fn fazer_login(st: St<'_>) -> Result<(), ApiError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let exe = st
        .deps()
        .claude_exe()
        .ok_or_else(|| ApiError::new("estado_invalido", "Claude Code não instalado"))?;
    std::process::Command::new("cmd")
        .arg("/k")
        .arg(exe)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map_err(|e| falha(format!("falha ao abrir o terminal: {e}")))?;
    Ok(())
}

#[tauri::command]
pub async fn sair(app: AppHandle, st: St<'_>) -> Result<(), ApiError> {
    bloq(&st, |a| a.encerrar()).await?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn passos(st: St<'_>, id: String) -> Result<Vec<PassoUi>, ApiError> {
    bloq(&st, move |a| a.passos(&id)).await
}

#[tauri::command]
pub async fn imagens(st: St<'_>, id: String) -> Result<Vec<ImagemUi>, ApiError> {
    bloq(&st, move |a| a.imagens(&id)).await
}

/// Bytes crus: chegam à UI como ArrayBuffer (sem base64).
#[tauri::command]
pub async fn miniatura(
    st: St<'_>,
    id: String,
    imagem: String,
) -> Result<tauri::ipc::Response, ApiError> {
    bloq(&st, move |a| a.miniatura(&id, &imagem))
        .await
        .map(tauri::ipc::Response::new)
}

#[tauri::command]
pub async fn definir_imagem(
    st: St<'_>,
    id: String,
    passo: usize,
    imagem: Option<String>,
) -> Result<(), ApiError> {
    bloq(&st, move |a| {
        a.definir_imagem(&id, passo, imagem.as_deref())
    })
    .await
}

// ponytail: bytes vão como array JSON; um print de ~1 MB é ok, troque por ipc::Request se 20 MB pesar
#[tauri::command]
pub async fn adicionar_imagem(st: St<'_>, id: String, bytes: Vec<u8>) -> Result<String, ApiError> {
    bloq(&st, move |a| a.adicionar_imagem(&id, &bytes)).await
}

#[tauri::command]
pub async fn republicar(st: St<'_>, id: String, sobrescrever: bool) -> Result<String, ApiError> {
    bloq(&st, move |a| a.republicar(&id, sobrescrever)).await
}

#[tauri::command]
pub async fn perguntas(st: St<'_>, id: String) -> Result<Option<Perguntas>, ApiError> {
    bloq(&st, move |a| a.perguntas(&id)).await
}

#[tauri::command]
pub async fn responder(
    st: St<'_>,
    id: String,
    respostas: Respostas,
) -> Result<AgentOutcome, ApiError> {
    bloq(&st, move |a| a.responder(&id, &respostas)).await
}

#[tauri::command]
pub async fn pular(st: St<'_>, id: String) -> Result<AgentOutcome, ApiError> {
    bloq(&st, move |a| a.pular(&id)).await
}

#[tauri::command]
pub async fn cancelar_perguntas(st: St<'_>, id: String) -> Result<(), ApiError> {
    bloq(&st, move |a| a.cancelar_perguntas(&id)).await
}
