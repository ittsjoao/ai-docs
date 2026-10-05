//! Adapters reais do app (composition root, spec §4).
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use screenmanual_agent::{check_claude, find_claude, install_skill, ClaudeAgent};
use screenmanual_app::Deps;
use screenmanual_capture::WinRecorder;
use screenmanual_core::domain::{TranscribeConfig, TranscriptionModel};
use screenmanual_outline::Outline;
use screenmanual_settings::{ensure_model, load_token, model_info, save_token, AppConfig, Paths};
use screenmanual_store::{FsStore, ImageCrops};
use screenmanual_whisper::WhisperTranscriber;

pub struct RealDeps {
    pub paths: Paths,
    store: FsStore,
    /// `claude.exe` achado no PATH ou em ~/.local/bin (para o "Fazer login").
    claude_exe: Option<PathBuf>,
    /// Versão, ou o motivo de o Claude Code não estar disponível.
    claude: Result<String, String>,
    /// Pasta do screenmanual-cli.exe: a mesma do executável do app (sidecar no plano 07).
    cli_dir: PathBuf,
}

impl RealDeps {
    /// Instala a skill e procura o Claude Code (spec §8, plano 05 "Para o plano 06").
    pub fn new() -> Result<Self> {
        let paths = Paths::from_env()?;
        std::fs::create_dir_all(&paths.sessions)
            .with_context(|| format!("falha ao criar {}", paths.sessions.display()))?;
        let home =
            PathBuf::from(std::env::var_os("USERPROFILE").context("USERPROFILE não definida")?);
        let claude_exe = find_claude(&std::env::var_os("PATH").unwrap_or_default(), &home);
        let claude = match (&claude_exe, install_skill(&home)) {
            (_, Err(e)) => Err(format!("falha ao instalar a skill /gerar-manual: {e:#}")),
            (None, _) => Err("Claude Code não encontrado no PATH nem em ~/.local/bin".into()),
            (Some(exe), Ok(_)) => check_claude(exe).map_err(|e| format!("{e:#}")),
        };
        let cli_dir = std::env::current_exe()?
            .parent()
            .context("pasta do executável inesperada")?
            .to_path_buf();
        Ok(Self {
            store: FsStore::new(&paths.sessions),
            paths,
            claude_exe,
            claude,
            cli_dir,
        })
    }

    pub fn claude_exe(&self) -> Option<&Path> {
        self.claude_exe.as_deref()
    }
}

impl Deps for RealDeps {
    type Store = FsStore;
    type Recorder = WinRecorder;
    type Transcriber = WhisperTranscriber;
    type Imaging = ImageCrops;
    type Wiki = Outline;
    type Agent = ClaudeAgent;

    fn store(&self) -> &FsStore {
        &self.store
    }
    fn recorder(&self) -> &WinRecorder {
        &WinRecorder
    }
    fn imaging(&self) -> &ImageCrops {
        &ImageCrops
    }
    fn claude(&self) -> Result<String, String> {
        self.claude.clone()
    }
    fn has_model(&self, m: TranscriptionModel) -> bool {
        let info = model_info(m);
        std::fs::metadata(self.paths.models_dir().join(info.file))
            .is_ok_and(|md| md.len() == info.size)
    }
    fn ensure_model(
        &self,
        m: TranscriptionModel,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<()> {
        ensure_model(&self.paths.models_dir(), m, progress).map(|_| ())
    }
    fn transcriber(&self, cfg: &TranscribeConfig) -> WhisperTranscriber {
        WhisperTranscriber::from_config(&self.paths.models_dir(), cfg)
    }
    fn wiki(&self, url: &str, token: &str) -> Result<Outline> {
        Outline::new(url, token)
    }
    fn agent(&self, cfg: &AppConfig, token: &str, cancel: Arc<AtomicBool>) -> Result<ClaudeAgent> {
        self.claude.as_ref().map_err(|e| anyhow!("{e}"))?;
        let exe = self
            .claude_exe
            .clone()
            .context("Claude Code não encontrado")?;
        if !self.cli_dir.join("screenmanual-cli.exe").is_file() {
            bail!(
                "screenmanual-cli.exe não está em {}; rode cargo build -p screenmanual-cli",
                self.cli_dir.display()
            );
        }
        let mut agent = ClaudeAgent::new(
            exe,
            self.cli_dir.clone(),
            &cfg.outline_url,
            token,
            &cfg.modelo_claude,
        );
        agent.cancel = cancel;
        Ok(agent)
    }
    fn token(&self) -> Result<Option<String>> {
        load_token()
    }
    fn save_token(&self, token: &str) -> Result<()> {
        save_token(token)
    }
}
