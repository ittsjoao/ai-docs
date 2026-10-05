//! `#[tauri::command]` finos: cada um chama o `App` numa thread de bloqueio (spec 2026-10-05 §3).
use screenmanual_app::{ApiError, App, Deps, Detalhe, EstadoGravacao, Inicio};
use screenmanual_core::domain::TranscriptionModel;
use screenmanual_core::ports::{AgentResult, Collection, SessionStore};
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
    instrucao: String,
    sobrescrever: bool,
) -> Result<AgentResult, ApiError> {
    bloq(&st, move |a| {
        a.gerar(&id, &colecao, &instrucao, sobrescrever)
    })
    .await
}

#[tauri::command]
pub async fn melhorar(
    st: St<'_>,
    id: String,
    texto: String,
    instrucao: String,
    sobrescrever: bool,
) -> Result<AgentResult, ApiError> {
    bloq(&st, move |a| {
        a.melhorar(&id, &texto, &instrucao, sobrescrever)
    })
    .await
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
    let invalido = || ApiError::new("estado_invalido", "sessão inválida");
    // Um único componente normal: recusa "C:", "..", barras e caminhos absolutos.
    let mut comps = std::path::Path::new(&id).components();
    if !matches!(
        (comps.next(), comps.next()),
        (Some(std::path::Component::Normal(_)), None)
    ) {
        return Err(invalido());
    }
    let dir = st.deps().store().dir(&id);
    if !dir.is_dir() {
        return Err(invalido());
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
