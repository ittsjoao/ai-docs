//! Orquestração do app (spec 2026-10-05 §3): estado em memória, locks e eventos, sem Tauri.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use screenmanual_agent::LoginRequired;
use screenmanual_core::commands::{process_session, start_recording, stop_recording, CommandError};
use screenmanual_core::domain::{Activity, TranscribeConfig, TranscriptionModel};
use screenmanual_core::ports::{
    AgentResult, Collection, Imaging, ManualAgent, Recorder, RecordingHandle, SessionStore,
    Transcriber, Validacao, Wiki,
};
use screenmanual_core::queries::{get_session, list_sessions, SessionSummary};
use screenmanual_settings::AppConfig;
use serde::Serialize;

/// Os adapters do app; `RealDeps` no bin, fakes nos testes.
pub trait Deps: Send + Sync + 'static {
    type Store: SessionStore + Send + Sync;
    type Recorder: Recorder + Send + Sync;
    type Transcriber: Transcriber;
    type Imaging: Imaging + Send + Sync;
    type Wiki: Wiki;
    type Agent: ManualAgent;

    fn store(&self) -> &Self::Store;
    fn recorder(&self) -> &Self::Recorder;
    fn imaging(&self) -> &Self::Imaging;
    /// Versão do Claude Code, ou por que ele não está disponível.
    fn claude(&self) -> Result<String, String>;
    fn has_model(&self, m: TranscriptionModel) -> bool;
    /// Baixa o modelo se faltar (sha256 conferido).
    fn ensure_model(
        &self,
        m: TranscriptionModel,
        progress: &mut dyn FnMut(u64, u64),
    ) -> anyhow::Result<()>;
    fn transcriber(&self, cfg: &TranscribeConfig) -> Self::Transcriber;
    fn wiki(&self, url: &str, token: &str) -> anyhow::Result<Self::Wiki>;
    /// Novo a cada geração (A6), com o token e a config atuais.
    fn agent(
        &self,
        cfg: &AppConfig,
        token: &str,
        cancel: Arc<AtomicBool>,
    ) -> anyhow::Result<Self::Agent>;
    fn token(&self) -> anyhow::Result<Option<String>>;
    fn save_token(&self, token: &str) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoGravacao {
    Gravando,
    Pausado,
    Parado,
}

/// Vai para a UI pelo canal `app` (A8).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "evento", rename_all = "snake_case")]
pub enum Evento {
    Sessao {
        id: String,
    },
    Progresso {
        id: String,
        texto: String,
    },
    Modelo {
        modelo: TranscriptionModel,
        baixado: u64,
        total: u64,
    },
    ModeloFim {
        modelo: TranscriptionModel,
        erro: Option<String>,
    },
    Gravacao {
        estado: EstadoGravacao,
        id: String,
    },
    /// Fim de processar/gerar/melhorar; o bin notifica se a janela estiver escondida.
    Fim {
        id: String,
        titulo: String,
        url: Option<String>,
        erro: Option<String>,
    },
}

/// Erro serializável para a UI. `kind`: editado_manualmente | login | ocupado | estado_invalido | outro.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApiError {
    pub kind: &'static str,
    pub mensagem: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<u64>,
}

impl ApiError {
    pub fn new(kind: &'static str, mensagem: impl Into<String>) -> Self {
        Self {
            kind,
            mensagem: mensagem.into(),
            local: None,
            remote: None,
        }
    }
}

fn is_login(e: &anyhow::Error) -> bool {
    e.downcast_ref::<LoginRequired>().is_some()
}

impl From<CommandError> for ApiError {
    fn from(e: CommandError) -> Self {
        if let CommandError::Unknown(inner) = e {
            return Self::from(inner);
        }
        let mensagem = e.to_string();
        match e {
            CommandError::EditedManually { local, remote } => Self {
                kind: "editado_manualmente",
                mensagem,
                local: Some(local),
                remote: Some(remote),
            },
            CommandError::Render(_) => Self::new("outro", mensagem),
            _ => Self::new("estado_invalido", mensagem),
        }
    }
}

/// `{:#}` mostra a cadeia inteira de contextos; `LoginRequired` em qualquer ponto vira `login`.
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self::new(
            if is_login(&e) { "login" } else { "outro" },
            format!("{e:#}"),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModeloStatus {
    pub modelo: TranscriptionModel,
    pub baixado: bool,
}

/// O que a UI precisa ao abrir.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Inicio {
    pub claude_versao: Option<String>,
    pub claude_erro: Option<String>,
    /// URL do Outline e token presentes.
    pub configurado: bool,
    pub gravacao: EstadoGravacao,
    pub gravando: Option<String>,
    pub gerando: Option<String>,
    pub modelo_atual: TranscriptionModel,
    pub modelos: Vec<ModeloStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Detalhe {
    #[serde(flatten)]
    pub resumo: SessionSummary,
    pub candidatos: Option<usize>,
    pub colecao: Option<String>,
    /// `validacao[]` do último result.json.
    pub validacao: Vec<Validacao>,
    pub pasta: String,
}

type Handle<D> = <<D as Deps>::Recorder as Recorder>::Handle;

struct Gravando<H> {
    id: String,
    handle: H,
    pausado: bool,
}

struct Estado<H> {
    atividade: HashMap<String, Activity>,
    gravacao: Option<Gravando<H>>,
    /// Lock global de geração (spec §7.1): sessão e o Cancelar do agente.
    geracao: Option<(String, Arc<AtomicBool>)>,
}

pub struct App<D: Deps> {
    deps: D,
    config_file: PathBuf,
    emit: Box<dyn Fn(Evento) + Send + Sync>,
    st: Mutex<Estado<Handle<D>>>,
    /// Um download de modelo por vez; quem chega depois espera e encontra o arquivo pronto.
    modelo: Mutex<()>,
}

fn nome(a: Activity) -> &'static str {
    match a {
        Activity::Recording => "gravando",
        Activity::Processing => "processando",
        Activity::Generating => "gerando o manual",
    }
}

fn estado_gravacao(pausado: Option<bool>) -> EstadoGravacao {
    match pausado {
        None => EstadoGravacao::Parado,
        Some(true) => EstadoGravacao::Pausado,
        Some(false) => EstadoGravacao::Gravando,
    }
}

impl<D: Deps> App<D> {
    pub fn new(deps: D, config_file: PathBuf, emit: Box<dyn Fn(Evento) + Send + Sync>) -> Self {
        Self {
            deps,
            config_file,
            emit,
            st: Mutex::new(Estado {
                atividade: HashMap::new(),
                gravacao: None,
                geracao: None,
            }),
            modelo: Mutex::new(()),
        }
    }

    pub fn deps(&self) -> &D {
        &self.deps
    }

    fn st(&self) -> MutexGuard<'_, Estado<Handle<D>>> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn progresso(&self, id: &str, texto: &str) {
        (self.emit)(Evento::Progresso {
            id: id.into(),
            texto: texto.into(),
        });
    }

    fn titulo(&self, id: &str) -> String {
        self.deps
            .store()
            .meta(id)
            .map(|m| m.title)
            .unwrap_or_else(|_| id.to_string())
    }

    pub fn config(&self) -> Result<AppConfig, ApiError> {
        Ok(AppConfig::load(&self.config_file)?)
    }

    pub fn salvar_config(&self, cfg: &AppConfig) -> Result<(), ApiError> {
        Ok(cfg.save(&self.config_file)?)
    }

    pub fn ocupado(&self) -> bool {
        let st = self.st();
        st.gravacao.is_some() || st.geracao.is_some()
    }

    pub fn inicio(&self) -> Result<Inicio, ApiError> {
        let cfg = self.config()?;
        let configurado = !cfg.outline_url.is_empty() && self.deps.token()?.is_some();
        let claude = self.deps.claude();
        let st = self.st();
        Ok(Inicio {
            claude_versao: claude.as_ref().ok().cloned(),
            claude_erro: claude.err(),
            configurado,
            gravacao: estado_gravacao(st.gravacao.as_ref().map(|g| g.pausado)),
            gravando: st.gravacao.as_ref().map(|g| g.id.clone()),
            gerando: st.geracao.as_ref().map(|g| g.0.clone()),
            modelo_atual: cfg.transcricao.modelo,
            modelos: [
                TranscriptionModel::Rapido,
                TranscriptionModel::Equilibrado,
                TranscriptionModel::Preciso,
            ]
            .map(|modelo| ModeloStatus {
                modelo,
                baixado: self.deps.has_model(modelo),
            })
            .to_vec(),
        })
    }

    pub fn sessoes(&self) -> Result<Vec<SessionSummary>, ApiError> {
        let atividade = self.st().atividade.clone();
        Ok(list_sessions(self.deps.store(), &|id| {
            atividade.get(id).copied()
        })?)
    }

    pub fn detalhe(&self, id: &str) -> Result<Detalhe, ApiError> {
        let store = self.deps.store();
        let atividade = self.st().atividade.get(id).copied();
        let resumo = get_session(store, id, atividade)?;
        let dir = store.dir(id);
        let candidatos = if dir.join("candidates.json").is_file() {
            Some(store.candidates(id)?.len())
        } else {
            None
        };
        let validacao = store
            .read_file(id, "result.json")
            .ok()
            .and_then(|b| serde_json::from_slice::<AgentResult>(&b).ok())
            .map(|r| r.validacao)
            .unwrap_or_default();
        Ok(Detalhe {
            resumo,
            candidatos,
            colecao: store.publish_state(id)?.map(|p| p.collection_id),
            validacao,
            pasta: dir.display().to_string(),
        })
    }

    /// Testa URL e token com `collections.list`; só grava se der certo. Token vazio = o guardado.
    pub fn conectar_outline(&self, url: &str, token: &str) -> Result<Vec<Collection>, ApiError> {
        let url = url.trim().trim_end_matches('/');
        if url.is_empty() {
            return Err(ApiError::new("estado_invalido", "informe a URL do Outline"));
        }
        let token = match token.trim() {
            "" => self
                .deps
                .token()?
                .ok_or_else(|| ApiError::new("estado_invalido", "informe o token do Outline"))?,
            t => t.to_string(),
        };
        let colecoes = self.deps.wiki(url, &token)?.collections()?;
        self.deps.save_token(&token)?;
        let mut cfg = self.config()?;
        cfg.outline_url = url.to_string();
        self.salvar_config(&cfg)?;
        Ok(colecoes)
    }

    fn credenciais(&self) -> Result<(AppConfig, String), ApiError> {
        let cfg = self.config()?;
        match self.deps.token()? {
            Some(t) if !cfg.outline_url.is_empty() => Ok((cfg, t)),
            _ => Err(ApiError::new(
                "estado_invalido",
                "configure o Outline (URL e token) nas Configurações",
            )),
        }
    }

    pub fn colecoes(&self) -> Result<Vec<Collection>, ApiError> {
        let (cfg, token) = self.credenciais()?;
        Ok(self.deps.wiki(&cfg.outline_url, &token)?.collections()?)
    }

    fn garantir_modelo(
        &self,
        m: TranscriptionModel,
        progress: &mut dyn FnMut(u64, u64),
    ) -> anyhow::Result<()> {
        let _vez = self.modelo.lock().unwrap_or_else(|e| e.into_inner());
        self.deps.ensure_model(m, progress)
    }

    pub fn baixar_modelo(&self, modelo: TranscriptionModel) -> Result<(), ApiError> {
        let mut ultimo = u64::MAX;
        let r = self.garantir_modelo(modelo, &mut |baixado, total| {
            let pct = baixado * 100 / total.max(1);
            if pct != ultimo {
                ultimo = pct;
                (self.emit)(Evento::Modelo {
                    modelo,
                    baixado,
                    total,
                });
            }
        });
        (self.emit)(Evento::ModeloFim {
            modelo,
            erro: r.as_ref().err().map(|e| format!("{e:#}")),
        });
        Ok(r?)
    }

    pub fn gravar(&self, titulo: &str, quando: &str) -> Result<String, ApiError> {
        if titulo.trim().is_empty() {
            return Err(ApiError::new(
                "estado_invalido",
                "diga o que você vai documentar",
            ));
        }
        let cfg = self.config()?;
        let mut st = self.st();
        if st.gravacao.is_some() {
            return Err(ApiError::new("ocupado", "já há uma gravação em andamento"));
        }
        let (id, handle) = start_recording(
            self.deps.store(),
            self.deps.recorder(),
            titulo,
            quando,
            &cfg.captura,
        )?;
        st.atividade.insert(id.clone(), Activity::Recording);
        st.gravacao = Some(Gravando {
            id: id.clone(),
            handle,
            pausado: false,
        });
        drop(st);
        (self.emit)(Evento::Gravacao {
            estado: EstadoGravacao::Gravando,
            id: id.clone(),
        });
        (self.emit)(Evento::Sessao { id: id.clone() });
        Ok(id)
    }

    fn sem_gravacao() -> ApiError {
        ApiError::new("estado_invalido", "nenhuma gravação em andamento")
    }

    /// Alterna pausa e retomada.
    pub fn pausar(&self) -> Result<EstadoGravacao, ApiError> {
        let mut st = self.st();
        let g = st.gravacao.as_mut().ok_or_else(Self::sem_gravacao)?;
        if g.pausado {
            g.handle.resume();
        } else {
            g.handle.pause();
        }
        g.pausado = !g.pausado;
        let (estado, id) = (estado_gravacao(Some(g.pausado)), g.id.clone());
        drop(st);
        (self.emit)(Evento::Gravacao { estado, id });
        Ok(estado)
    }

    pub fn marcar(&self) -> Result<(), ApiError> {
        match self.st().gravacao.as_ref() {
            Some(g) if !g.pausado => {
                g.handle.marker();
                Ok(())
            }
            Some(_) => Err(ApiError::new("estado_invalido", "a gravação está pausada")),
            None => Err(Self::sem_gravacao()),
        }
    }

    /// Para e processa (D10).
    pub fn parar(&self) -> Result<String, ApiError> {
        // Recording -> Processing sem instante livre: a sessão nunca fica sem Activity.
        let id = self.parar_gravacao(Some(Activity::Processing))?;
        let r = self.processar_dentro(&id, false);
        self.liberar(&id, r.as_ref().err(), None);
        r?;
        Ok(id)
    }

    /// Para a gravação; `depois` é a Activity que substitui `Recording` sob o mesmo lock
    /// (`None` = sem Activity). Se o stop falhar, a sessão é liberada.
    fn parar_gravacao(&self, depois: Option<Activity>) -> Result<String, ApiError> {
        let g = {
            let mut st = self.st();
            let g = st.gravacao.take().ok_or_else(Self::sem_gravacao)?;
            match depois {
                Some(a) => st.atividade.insert(g.id.clone(), a),
                None => st.atividade.remove(&g.id),
            };
            g
        };
        (self.emit)(Evento::Gravacao {
            estado: EstadoGravacao::Parado,
            id: g.id.clone(),
        });
        let r = stop_recording(self.deps.store(), &g.id, g.handle);
        if r.is_err() {
            self.st().atividade.remove(&g.id);
        }
        (self.emit)(Evento::Sessao { id: g.id.clone() });
        r?;
        Ok(g.id)
    }

    #[allow(dead_code)] // usado pelo encerrar (Task 3): para sem processar
    fn parar_sem_processar(&self) -> Result<String, ApiError> {
        self.parar_gravacao(None)
    }

    fn ocupar(&self, id: &str, a: Activity) -> Result<(), ApiError> {
        let mut st = self.st();
        if let Some(atual) = st.atividade.get(id) {
            return Err(ApiError::new(
                "ocupado",
                format!("a sessão já está {}", nome(*atual)),
            ));
        }
        st.atividade.insert(id.to_string(), a);
        drop(st);
        (self.emit)(Evento::Sessao { id: id.into() });
        Ok(())
    }

    fn liberar(&self, id: &str, erro: Option<&ApiError>, url: Option<String>) {
        {
            let mut st = self.st();
            st.atividade.remove(id);
            if st.geracao.as_ref().is_some_and(|g| g.0 == id) {
                st.geracao = None;
            }
        }
        (self.emit)(Evento::Sessao { id: id.into() });
        (self.emit)(Evento::Fim {
            id: id.into(),
            titulo: self.titulo(id),
            url,
            erro: erro.map(|e| e.mensagem.clone()),
        });
    }

    /// Transcreve (baixando o modelo se preciso), monta candidatos e crops.
    pub fn processar(&self, id: &str, refazer: bool) -> Result<usize, ApiError> {
        self.ocupar(id, Activity::Processing)?;
        let r = self.processar_dentro(id, refazer);
        self.liberar(id, r.as_ref().err(), None);
        r
    }

    fn processar_dentro(&self, id: &str, refazer: bool) -> Result<usize, ApiError> {
        let cfg = self.config()?;
        let store = self.deps.store();
        let transcrever =
            store.audio_path(id)?.is_some() && (refazer || store.transcript(id)?.is_none());
        if transcrever {
            let mut ultimo = u64::MAX;
            self.garantir_modelo(cfg.transcricao.modelo, &mut |baixado, total| {
                let pct = baixado * 100 / total.max(1);
                if pct != ultimo {
                    ultimo = pct;
                    self.progresso(id, &format!("baixando o modelo de transcrição: {pct}%"));
                }
            })?;
            self.progresso(id, "transcrevendo o áudio e montando os passos");
        } else {
            self.progresso(id, "montando os passos");
        }
        let t = self.deps.transcriber(&cfg.transcricao);
        Ok(process_session(
            store,
            &t,
            self.deps.imaging(),
            id,
            &cfg.build,
            refazer,
        )?)
    }
}
