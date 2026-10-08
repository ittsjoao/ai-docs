use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::*;

pub type PortResult<T> = anyhow::Result<T>;

/// Pasta da sessão (spec §3). Caminhos `rel` são relativos à pasta, com `/`.
pub trait SessionStore {
    fn dir(&self, id: &str) -> PathBuf;
    fn list_ids(&self) -> PortResult<Vec<String>>;
    fn create(&self, meta: &SessionMeta) -> PortResult<()>;
    fn meta(&self, id: &str) -> PortResult<SessionMeta>;
    fn save_meta(&self, meta: &SessionMeta) -> PortResult<()>;
    fn events(&self, id: &str) -> PortResult<Vec<Event>>;
    fn audio_path(&self, id: &str) -> PortResult<Option<PathBuf>>;
    fn transcript(&self, id: &str) -> PortResult<Option<Vec<Segment>>>;
    fn save_transcript(&self, id: &str, segments: &[Segment]) -> PortResult<()>;
    fn candidates(&self, id: &str) -> PortResult<Vec<Candidate>>;
    fn save_candidates(&self, id: &str, candidates: &[Candidate]) -> PortResult<()>;
    fn manual(&self, id: &str) -> PortResult<Option<Manual>>;
    fn save_manual(&self, id: &str, manual: &Manual) -> PortResult<()>;
    /// Decodifica PNG/JPEG, grava como `crops/u###.png` (próximo número livre) e devolve `u###`.
    fn add_image(&self, id: &str, bytes: &[u8]) -> PortResult<String>;
    /// Grava manual.md e copia cada `images[i].from` para `images[i].to`.
    fn save_rendered(&self, id: &str, rendered: &Rendered) -> PortResult<()>;
    fn read_file(&self, id: &str, rel: &str) -> PortResult<Vec<u8>>;
    /// Grava published/manual.md e published/img/<attachment_id>.png.
    fn save_published(
        &self,
        id: &str,
        markdown: &str,
        images: &[(String, Vec<u8>)],
    ) -> PortResult<()>;
    fn publish_state(&self, id: &str) -> PortResult<Option<PublishState>>;
    fn save_publish_state(&self, id: &str, state: &PublishState) -> PortResult<()>;
    /// Acrescenta uma linha {t, texto} a feedback.jsonl (o adapter carimba a hora).
    fn append_feedback(&self, id: &str, text: &str) -> PortResult<()>;
    /// Contrato que o adapter de disco (plano 03) deve reproduzir:
    /// `ended` = events.jsonl contém um evento `session_end`; `has_candidates` = candidates.json existe;
    /// `publish` = conteúdo de publish.json; `error` = mensagem gravada por `set_error`
    /// (persistida na pasta da sessão, ex.: `error.txt`) e limpa com `set_error(None)`.
    fn facts(&self, id: &str) -> PortResult<SessionFacts>;
    fn set_error(&self, id: &str, message: Option<&str>) -> PortResult<()>;
}

pub trait Transcriber {
    /// Segmentos com tempos relativos ao início do áudio.
    fn transcribe(&self, audio: &Path, prompt: &str) -> PortResult<Vec<Segment>>;
}

pub trait Imaging {
    fn render_crops(&self, dir: &Path, specs: &[CropSpec]) -> PortResult<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMode {
    Gerar,
    Melhoria,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Validacao {
    pub tipo: String,
    pub detalhe: String,
}

/// Conteúdo de result.json escrito pela skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentResult {
    pub url: String,
    pub revision: u64,
    pub rodadas: u32,
    #[serde(default)]
    pub validacao: Vec<Validacao>,
}

pub trait ManualAgent {
    fn run(
        &self,
        dir: &Path,
        mode: AgentMode,
        progress: &mut dyn FnMut(&str),
    ) -> PortResult<AgentResult>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocInfo {
    pub id: String,
    pub url: String,
    pub revision: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
}

/// Nó da árvore de documentos de uma coleção (`collections.documents`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocNode {
    pub id: String,
    pub title: String,
    pub children: Vec<DocNode>,
}

pub trait Wiki {
    fn collections(&self) -> PortResult<Vec<Collection>>;
    /// Árvore inteira de documentos da coleção.
    fn documents(&self, collection_id: &str) -> PortResult<Vec<DocNode>>;
    fn upload_image(&self, doc_id: Option<&str>, name: &str, bytes: &[u8]) -> PortResult<String>;
    fn create_draft(
        &self,
        collection_id: &str,
        parent: Option<&str>,
        title: &str,
        icon: &str,
        text: &str,
    ) -> PortResult<DocInfo>;
    fn update(&self, id: &str, title: &str, icon: &str, text: &str) -> PortResult<DocInfo>;
    fn info(&self, id: &str) -> PortResult<DocInfo>;
    fn publish(&self, id: &str) -> PortResult<DocInfo>;
    fn download_attachment(&self, attachment_id: &str) -> PortResult<Vec<u8>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopInfo {
    pub duration_ms: u64,
    pub audio_offset_ms: Option<i64>,
}

pub trait RecordingHandle: Send {
    fn pause(&self);
    fn resume(&self);
    fn marker(&self);
    fn stop(self) -> PortResult<StopInfo>;
}

pub trait Recorder {
    type Handle: RecordingHandle;
    fn start(&self, dir: &Path, cfg: &CaptureConfig) -> PortResult<Self::Handle>;
}
