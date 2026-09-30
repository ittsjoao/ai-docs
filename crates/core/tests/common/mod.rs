#![allow(dead_code)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use screenmanual_core::domain::*;
use screenmanual_core::ports::*;

/// markdown + (attachment id, bytes)
pub type Published = (String, Vec<(String, Vec<u8>)>);

#[derive(Default, Clone)]
pub struct Sess {
    pub meta: Option<SessionMeta>,
    pub events: Vec<Event>,
    pub has_audio: bool,
    pub transcript: Option<Vec<Segment>>,
    pub candidates: Option<Vec<Candidate>>,
    pub manual: Option<Manual>,
    pub markdown: Option<String>,
    pub files: HashMap<String, Vec<u8>>,
    pub published: Option<Published>,
    pub publish: Option<PublishState>,
    pub feedback: Vec<String>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct FakeStore {
    pub s: Mutex<HashMap<String, Sess>>,
}

impl FakeStore {
    pub fn with(id: &str, sess: Sess) -> Self {
        let store = FakeStore::default();
        store.s.lock().unwrap().insert(id.to_string(), sess);
        store
    }
    pub fn get(&self, id: &str) -> Sess {
        self.s.lock().unwrap().get(id).cloned().unwrap_or_default()
    }
    fn edit<T>(&self, id: &str, f: impl FnOnce(&mut Sess) -> T) -> T {
        f(self.s.lock().unwrap().entry(id.to_string()).or_default())
    }
}

impl SessionStore for FakeStore {
    fn dir(&self, id: &str) -> PathBuf {
        PathBuf::from(id)
    }
    fn list_ids(&self) -> Result<Vec<String>> {
        Ok(self.s.lock().unwrap().keys().cloned().collect())
    }
    fn create(&self, meta: &SessionMeta) -> Result<()> {
        self.edit(&meta.id, |s| s.meta = Some(meta.clone()));
        Ok(())
    }
    fn meta(&self, id: &str) -> Result<SessionMeta> {
        self.get(id).meta.ok_or_else(|| anyhow!("session.json ausente"))
    }
    fn save_meta(&self, meta: &SessionMeta) -> Result<()> {
        self.create(meta)
    }
    fn events(&self, id: &str) -> Result<Vec<Event>> {
        Ok(self.get(id).events)
    }
    fn audio_path(&self, id: &str) -> Result<Option<PathBuf>> {
        Ok(self.get(id).has_audio.then(|| PathBuf::from(id).join("audio.wav")))
    }
    fn transcript(&self, id: &str) -> Result<Option<Vec<Segment>>> {
        Ok(self.get(id).transcript)
    }
    fn save_transcript(&self, id: &str, segments: &[Segment]) -> Result<()> {
        self.edit(id, |s| s.transcript = Some(segments.to_vec()));
        Ok(())
    }
    fn candidates(&self, id: &str) -> Result<Vec<Candidate>> {
        self.get(id).candidates.ok_or_else(|| anyhow!("candidates.json ausente"))
    }
    fn save_candidates(&self, id: &str, candidates: &[Candidate]) -> Result<()> {
        self.edit(id, |s| s.candidates = Some(candidates.to_vec()));
        Ok(())
    }
    fn manual(&self, id: &str) -> Result<Option<Manual>> {
        Ok(self.get(id).manual)
    }
    fn save_rendered(&self, id: &str, rendered: &Rendered) -> Result<()> {
        self.edit(id, |s| -> Result<()> {
            for img in &rendered.images {
                let bytes = s.files.get(&img.from).cloned().ok_or_else(|| anyhow!("crop ausente: {}", img.from))?;
                s.files.insert(img.to.clone(), bytes);
            }
            s.markdown = Some(rendered.markdown.clone());
            Ok(())
        })
    }
    fn read_file(&self, id: &str, rel: &str) -> Result<Vec<u8>> {
        self.get(id).files.get(rel).cloned().ok_or_else(|| anyhow!("arquivo ausente: {rel}"))
    }
    fn save_published(&self, id: &str, markdown: &str, images: &[(String, Vec<u8>)]) -> Result<()> {
        self.edit(id, |s| s.published = Some((markdown.to_string(), images.to_vec())));
        Ok(())
    }
    fn publish_state(&self, id: &str) -> Result<Option<PublishState>> {
        Ok(self.get(id).publish)
    }
    fn save_publish_state(&self, id: &str, state: &PublishState) -> Result<()> {
        self.edit(id, |s| s.publish = Some(state.clone()));
        Ok(())
    }
    fn append_feedback(&self, id: &str, text: &str) -> Result<()> {
        self.edit(id, |s| s.feedback.push(text.to_string()));
        Ok(())
    }
    fn facts(&self, id: &str) -> Result<SessionFacts> {
        let s = self.get(id);
        Ok(SessionFacts {
            ended: s.events.iter().any(|e| matches!(e, Event::SessionEnd { .. })),
            has_candidates: s.candidates.is_some(),
            publish: s.publish,
            error: s.error,
        })
    }
    fn set_error(&self, id: &str, message: Option<&str>) -> Result<()> {
        self.edit(id, |s| s.error = message.map(str::to_string));
        Ok(())
    }
}

pub struct FakeTranscriber {
    pub out: Result<Vec<Segment>, String>,
    pub prompts: Mutex<Vec<String>>,
}

impl FakeTranscriber {
    pub fn ok(out: Vec<Segment>) -> Self {
        Self { out: Ok(out), prompts: Mutex::default() }
    }
    pub fn failing(msg: &str) -> Self {
        Self { out: Err(msg.to_string()), prompts: Mutex::default() }
    }
    pub fn calls(&self) -> usize {
        self.prompts.lock().unwrap().len()
    }
}

impl Transcriber for FakeTranscriber {
    fn transcribe(&self, _audio: &Path, prompt: &str) -> Result<Vec<Segment>> {
        self.prompts.lock().unwrap().push(prompt.to_string());
        self.out.clone().map_err(|e| anyhow!(e))
    }
}

#[derive(Default)]
pub struct FakeImaging {
    pub specs: Mutex<Vec<CropSpec>>,
}

impl Imaging for FakeImaging {
    fn render_crops(&self, _dir: &Path, specs: &[CropSpec]) -> Result<()> {
        self.specs.lock().unwrap().extend_from_slice(specs);
        Ok(())
    }
}

pub struct FakeAgent {
    pub result: Result<AgentResult, String>,
    pub modes: Mutex<Vec<AgentMode>>,
}

impl FakeAgent {
    pub fn ok() -> Self {
        Self { result: Ok(AgentResult { url: "https://wiki/doc/doc-1".into(), revision: 2, rodadas: 1, validacao: vec![] }), modes: Mutex::default() }
    }
    pub fn failing(msg: &str) -> Self {
        Self { result: Err(msg.to_string()), modes: Mutex::default() }
    }
}

impl ManualAgent for FakeAgent {
    fn run(&self, _dir: &Path, mode: AgentMode, progress: &mut dyn FnMut(&str)) -> Result<AgentResult> {
        progress("trabalhando");
        self.modes.lock().unwrap().push(mode);
        self.result.clone().map_err(|e| anyhow!(e))
    }
}

/// docs: id → (título, texto, revisão, publicado)
#[derive(Default)]
pub struct FakeWiki {
    pub docs: Mutex<HashMap<String, (String, String, u64, bool)>>,
    pub uploads: Mutex<Vec<(String, Vec<u8>)>>,
}

impl FakeWiki {
    pub fn external_edit(&self, id: &str) {
        self.docs.lock().unwrap().get_mut(id).unwrap().2 += 1;
    }
    pub fn is_published(&self, id: &str) -> bool {
        self.docs.lock().unwrap()[id].3
    }
    fn info_of(&self, id: &str) -> Result<DocInfo> {
        let docs = self.docs.lock().unwrap();
        let (_, text, revision, _) = docs.get(id).ok_or_else(|| anyhow!("doc não existe"))?;
        Ok(DocInfo { id: id.to_string(), url: format!("https://wiki/doc/{id}"), revision: *revision, text: text.clone() })
    }
    fn edit_doc(&self, id: &str, f: impl FnOnce(&mut (String, String, u64, bool))) -> Result<DocInfo> {
        {
            let mut docs = self.docs.lock().unwrap();
            let doc = docs.get_mut(id).ok_or_else(|| anyhow!("doc não existe"))?;
            f(doc);
            doc.2 += 1;
        }
        self.info_of(id)
    }
}

impl Wiki for FakeWiki {
    fn collections(&self) -> Result<Vec<Collection>> {
        Ok(vec![Collection { id: "col-1".into(), name: "Manuais".into() }])
    }
    fn upload_image(&self, _doc_id: Option<&str>, name: &str, bytes: &[u8]) -> Result<String> {
        let mut uploads = self.uploads.lock().unwrap();
        uploads.push((name.to_string(), bytes.to_vec()));
        Ok(format!("att-{}", uploads.len()))
    }
    fn create_draft(&self, _collection_id: &str, title: &str, text: &str) -> Result<DocInfo> {
        let id = format!("doc-{}", self.docs.lock().unwrap().len() + 1);
        self.docs.lock().unwrap().insert(id.clone(), (title.to_string(), text.to_string(), 1, false));
        self.info_of(&id)
    }
    fn update(&self, id: &str, title: &str, text: &str) -> Result<DocInfo> {
        self.edit_doc(id, |d| {
            d.0 = title.to_string();
            d.1 = text.to_string();
        })
    }
    fn info(&self, id: &str) -> Result<DocInfo> {
        self.info_of(id)
    }
    fn publish(&self, id: &str) -> Result<DocInfo> {
        self.edit_doc(id, |d| d.3 = true)
    }
    fn download_attachment(&self, attachment_id: &str) -> Result<Vec<u8>> {
        let n: usize = attachment_id.trim_start_matches("att-").parse()?;
        self.uploads.lock().unwrap().get(n - 1).map(|u| u.1.clone()).ok_or_else(|| anyhow!("anexo não existe"))
    }
}

pub struct FakeHandle {
    pub stopped: Arc<Mutex<bool>>,
}

impl RecordingHandle for FakeHandle {
    fn pause(&self) {}
    fn resume(&self) {}
    fn marker(&self) {}
    fn stop(self) -> Result<StopInfo> {
        *self.stopped.lock().unwrap() = true;
        Ok(StopInfo { duration_ms: 60_000, audio_offset_ms: Some(120) })
    }
}

#[derive(Default)]
pub struct FakeRecorder {
    pub started: Mutex<Vec<PathBuf>>,
}

impl Recorder for FakeRecorder {
    type Handle = FakeHandle;
    fn start(&self, dir: &Path, _cfg: &CaptureConfig) -> Result<FakeHandle> {
        self.started.lock().unwrap().push(dir.to_path_buf());
        Ok(FakeHandle { stopped: Arc::default() })
    }
}

pub fn meta(id: &str, title: &str, started_at: &str) -> SessionMeta {
    SessionMeta { schema_version: SCHEMA_VERSION, id: id.into(), title: title.into(), started_at: started_at.into(), audio_offset_ms: None, duration_ms: None }
}

pub fn cand(id: &str, crop: Option<&str>) -> Candidate {
    Candidate { id: id.into(), t: 0, kind: CandidateKind::Click, app: "erp.exe".into(), window: "ERP".into(), url: None, el: None, input: None, keys: vec![], speech: vec![], flags: vec![], crop: crop.map(str::to_string), context_shot: None }
}

pub fn manual_with_image(img: &str) -> Manual {
    Manual {
        schema_version: 1,
        titulo: "Emitir NFS-e".into(),
        objetivo: "Emitir nota.".into(),
        pre_requisitos: vec![],
        secoes: vec![Secao {
            titulo: "Cadastro".into(),
            passos: vec![Passo { candidatos: vec![img.into()], imagem: Some(img.into()), texto: "Clique em **Nova nota**.".into(), aviso: None, dica: None }],
        }],
        descartados: vec![],
    }
}

pub fn click_ev(t: u64) -> Event {
    Event::Click {
        t,
        button: MouseButton::Left,
        x: 500,
        y: 500,
        up_x: 500,
        up_y: 500,
        shot: Some(format!("shots/{t:08}.png")),
        dhash: None,
        el: None,
        monitor: Rect { left: 0, top: 0, right: 1920, bottom: 1080 },
    }
}
