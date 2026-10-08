#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Result};
use screenmanual_agent::LoginRequired;
use screenmanual_app::Deps;
use screenmanual_core::domain::{
    CaptureConfig, CropSpec, Segment, TranscribeConfig, TranscriptionModel,
};
use screenmanual_core::ports::*;
use screenmanual_settings::AppConfig;
use screenmanual_store::FsStore;

/// O que o agente falso faz no próximo `run`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Plano {
    Ok,
    Login,
    EsperaCancelar,
    Editado,
}

pub struct FakeDeps {
    pub store: FsStore,
    pub recorder: FakeRecorder,
    pub token: Mutex<Option<String>>,
    pub claude: Mutex<Result<String, String>>,
    /// Revisão do documento no Outline falso.
    pub remota: Arc<AtomicU64>,
    pub plano: Mutex<Plano>,
    pub baixados: Mutex<Vec<TranscriptionModel>>,
    /// `revision` do publish.json no início do último `run` do agente.
    pub revisao_vista: Arc<Mutex<Option<u64>>>,
}

impl FakeDeps {
    pub fn new(root: &Path) -> Self {
        std::fs::create_dir_all(root).unwrap();
        Self {
            store: FsStore::new(root),
            recorder: FakeRecorder::default(),
            token: Mutex::new(Some("tok".into())),
            claude: Mutex::new(Ok("2.1.289 (Claude Code)".into())),
            remota: Arc::new(AtomicU64::new(1)),
            plano: Mutex::new(Plano::Ok),
            baixados: Mutex::new(vec![]),
            revisao_vista: Arc::new(Mutex::new(None)),
        }
    }
}

impl Deps for FakeDeps {
    type Store = FsStore;
    type Recorder = FakeRecorder;
    type Transcriber = FakeTranscriber;
    type Imaging = FakeImaging;
    type Wiki = FakeWiki;
    type Agent = FakeAgent;

    fn store(&self) -> &FsStore {
        &self.store
    }
    fn recorder(&self) -> &FakeRecorder {
        &self.recorder
    }
    fn imaging(&self) -> &FakeImaging {
        &FakeImaging
    }
    fn claude(&self) -> Result<String, String> {
        self.claude.lock().unwrap().clone()
    }
    fn has_model(&self, m: TranscriptionModel) -> bool {
        self.baixados.lock().unwrap().contains(&m)
    }
    fn ensure_model(
        &self,
        m: TranscriptionModel,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<()> {
        progress(50, 100);
        progress(100, 100);
        let mut b = self.baixados.lock().unwrap();
        if !b.contains(&m) {
            b.push(m);
        }
        Ok(())
    }
    fn transcriber(&self, _cfg: &TranscribeConfig) -> FakeTranscriber {
        FakeTranscriber
    }
    fn wiki(&self, _url: &str, token: &str) -> Result<FakeWiki> {
        if token == "ruim" {
            bail!("401 não autorizado");
        }
        Ok(FakeWiki {
            remota: self.remota.clone(),
        })
    }
    fn agent(&self, _cfg: &AppConfig, _token: &str, cancel: Arc<AtomicBool>) -> Result<FakeAgent> {
        Ok(FakeAgent {
            plano: *self.plano.lock().unwrap(),
            cancel,
            remota: self.remota.clone(),
            revisao_vista: self.revisao_vista.clone(),
        })
    }
    fn token(&self) -> Result<Option<String>> {
        Ok(self.token.lock().unwrap().clone())
    }
    fn save_token(&self, token: &str) -> Result<()> {
        *self.token.lock().unwrap() = Some(token.to_string());
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeRecorder {
    pub chamadas: Arc<Mutex<Vec<&'static str>>>,
}

pub struct FakeHandle {
    dir: PathBuf,
    chamadas: Arc<Mutex<Vec<&'static str>>>,
}

impl Recorder for FakeRecorder {
    type Handle = FakeHandle;
    fn start(&self, dir: &Path, _cfg: &CaptureConfig) -> PortResult<FakeHandle> {
        std::fs::create_dir_all(dir)?;
        Ok(FakeHandle {
            dir: dir.to_path_buf(),
            chamadas: self.chamadas.clone(),
        })
    }
}

impl RecordingHandle for FakeHandle {
    fn pause(&self) {
        self.chamadas.lock().unwrap().push("pause");
    }
    fn resume(&self) {
        self.chamadas.lock().unwrap().push("resume");
    }
    fn marker(&self) {
        self.chamadas.lock().unwrap().push("marker");
    }
    fn stop(self) -> PortResult<StopInfo> {
        std::fs::write(
            self.dir.join("events.jsonl"),
            "{\"type\":\"session_end\",\"t\":900}\n",
        )?;
        Ok(StopInfo {
            duration_ms: 900,
            audio_offset_ms: None,
        })
    }
}

pub struct FakeTranscriber;

impl Transcriber for FakeTranscriber {
    fn transcribe(&self, _audio: &Path, _prompt: &str) -> PortResult<Vec<Segment>> {
        Ok(vec![])
    }
}

pub struct FakeImaging;

impl Imaging for FakeImaging {
    fn render_crops(&self, _dir: &Path, _specs: &[CropSpec]) -> PortResult<()> {
        Ok(())
    }
}

pub struct FakeWiki {
    remota: Arc<AtomicU64>,
}

fn doc(id: &str, revision: u64) -> DocInfo {
    DocInfo {
        id: id.into(),
        url: "http://wiki/doc/manual".into(),
        revision,
        text: String::new(),
    }
}

impl Wiki for FakeWiki {
    fn collections(&self) -> PortResult<Vec<Collection>> {
        Ok(vec![Collection {
            id: "col".into(),
            name: "Manuais".into(),
        }])
    }
    fn upload_image(&self, _doc: Option<&str>, _name: &str, _bytes: &[u8]) -> PortResult<String> {
        bail!("não usado")
    }
    fn documents(&self, _c: &str) -> PortResult<Vec<DocNode>> {
        Ok(vec![DocNode {
            id: "pai".into(),
            title: "Redes".into(),
            children: vec![DocNode {
                id: "filho".into(),
                title: "MikroTik".into(),
                children: vec![],
            }],
        }])
    }
    fn create_draft(
        &self,
        _c: &str,
        _p: Option<&str>,
        _t: &str,
        _i: &str,
        _x: &str,
    ) -> PortResult<DocInfo> {
        bail!("não usado")
    }
    fn update(&self, _id: &str, _t: &str, _i: &str, _x: &str) -> PortResult<DocInfo> {
        bail!("não usado")
    }
    fn info(&self, id: &str) -> PortResult<DocInfo> {
        Ok(doc(id, self.remota.load(SeqCst)))
    }
    fn publish(&self, id: &str) -> PortResult<DocInfo> {
        Ok(doc(id, self.remota.fetch_add(1, SeqCst) + 1))
    }
    fn download_attachment(&self, _id: &str) -> PortResult<Vec<u8>> {
        bail!("não usado")
    }
}

/// Faz o papel da skill: grava publish.json (como o CLI) e result.json.
pub struct FakeAgent {
    plano: Plano,
    cancel: Arc<AtomicBool>,
    remota: Arc<AtomicU64>,
    revisao_vista: Arc<Mutex<Option<u64>>>,
}

impl ManualAgent for FakeAgent {
    fn run(
        &self,
        dir: &Path,
        _mode: AgentMode,
        progress: &mut dyn FnMut(&str),
    ) -> PortResult<AgentResult> {
        progress("lendo");
        let publish = dir.join("publish.json");
        let mut p: serde_json::Value = serde_json::from_slice(&std::fs::read(&publish)?)?;
        *self.revisao_vista.lock().unwrap() = p["revision"].as_u64();
        let validacao = match self.plano {
            Plano::Login => return Err(LoginRequired.into()),
            Plano::EsperaCancelar => loop {
                if self.cancel.load(SeqCst) {
                    bail!("geração cancelada");
                }
                std::thread::sleep(Duration::from_millis(10));
            },
            Plano::Ok => serde_json::json!([{"tipo": "aviso", "detalhe": "passo 3 sem imagem"}]),
            Plano::Editado => {
                serde_json::json!([{"tipo": "editado_manualmente", "detalhe": "exit 3"}])
            }
        };
        let revision = self.remota.load(SeqCst);
        if self.plano == Plano::Ok {
            p["outline_id"] = "doc".into();
            p["url"] = "http://wiki/doc/manual".into();
            p["revision"] = revision.into();
            p["status"] = "draft".into();
            std::fs::write(&publish, serde_json::to_vec(&p)?)?;
        }
        let result = serde_json::json!({
            "url": "http://wiki/doc/manual", "revision": revision, "rodadas": 1, "validacao": validacao
        });
        std::fs::write(dir.join("result.json"), serde_json::to_vec(&result)?)?;
        Ok(serde_json::from_value(result)?)
    }
}
