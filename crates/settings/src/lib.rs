//! Configuração do app: caminhos, config.json e token do Outline (spec §6.1.8, §8, D14).
mod models;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use screenmanual_core::domain::{BuildConfig, CaptureConfig, TranscribeConfig};
use serde::{Deserialize, Serialize};

/// Onde o app guarda seus dados (spec §8).
#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
    /// `%LOCALAPPDATA%\screenManual`: config.json, models\, logs\
    pub data: PathBuf,
    /// `%USERPROFILE%\screenManual\sessions`
    pub sessions: PathBuf,
}

impl Paths {
    pub fn from_env() -> Result<Self> {
        let local = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA não definida")?;
        let home = std::env::var_os("USERPROFILE").context("USERPROFILE não definida")?;
        Ok(Self {
            data: PathBuf::from(local).join("screenManual"),
            sessions: PathBuf::from(home).join("screenManual").join("sessions"),
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.data.join("config.json")
    }

    pub fn models_dir(&self) -> PathBuf {
        self.data.join("models")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data.join("logs")
    }
}

/// `config.json`. Campo ausente = padrão, para o arquivo sobreviver a versões novas do app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Ex.: `https://wiki.auster.local`, sem barra final.
    pub outline_url: String,
    pub colecao_padrao: Option<String>,
    /// `--model` do `claude -p`; vazio = padrão da conta. Validação de 2026-10-04: o sonnet basta.
    pub modelo_claude: String,
    pub transcricao: TranscribeConfig,
    pub captura: CaptureConfig,
    pub build: BuildConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            outline_url: String::new(),
            colecao_padrao: None,
            modelo_claude: "sonnet".into(),
            transcricao: TranscribeConfig::default(),
            captura: CaptureConfig::default(),
            build: BuildConfig::default(),
        }
    }
}

impl AppConfig {
    /// Arquivo ausente = configuração padrão (1º uso).
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("config.json inválido em {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("falha ao ler {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("falha ao criar {}", dir.display()))?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)
            .with_context(|| format!("falha ao gravar {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("falha ao gravar {}", path.display()))
    }
}

const KEYRING_SERVICE: &str = "screenManual";
const KEYRING_USER: &str = "outline";

fn entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .context("Windows Credential Manager indisponível")
}

/// Token do Outline do operador (spec D14). `None` = ainda não configurado.
pub fn load_token() -> Result<Option<String>> {
    match entry()?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).context("falha ao ler o token do Outline no Credential Manager"),
    }
}

pub fn save_token(token: &str) -> Result<()> {
    entry()?
        .set_password(token.trim())
        .context("falha ao gravar o token do Outline no Credential Manager")
}

pub fn delete_token() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("falha ao apagar o token do Outline no Credential Manager"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenmanual_core::domain::TranscriptionModel;

    fn temp(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("smsettings-{name}-{nanos}"))
    }

    #[test]
    fn config_defaults_partial_files_and_roundtrip() {
        let path = temp("cfg").join("config.json");
        let d = AppConfig::load(&path).unwrap();
        assert_eq!(d, AppConfig::default());
        assert_eq!(d.modelo_claude, "sonnet");

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"outline_url":"https://wiki.x","transcricao":{"modelo":"rapido"}}"#,
        )
        .unwrap();
        let mut c = AppConfig::load(&path).unwrap();
        assert_eq!(c.outline_url, "https://wiki.x");
        assert_eq!(c.transcricao.modelo, TranscriptionModel::Rapido);
        assert_eq!(c.build, BuildConfig::default());
        assert_eq!(c.modelo_claude, "sonnet");

        c.colecao_padrao = Some("col-1".into());
        c.save(&path).unwrap();
        assert_eq!(AppConfig::load(&path).unwrap(), c);
        assert!(!path.with_extension("tmp").exists());

        std::fs::write(&path, "{").unwrap();
        assert!(AppConfig::load(&path)
            .unwrap_err()
            .to_string()
            .contains("config.json inválido"));
    }

    #[test]
    fn paths_follow_the_spec() {
        let p = Paths {
            data: PathBuf::from(r"C:\L\screenManual"),
            sessions: PathBuf::from(r"C:\U\screenManual\sessions"),
        };
        assert_eq!(
            p.config_file(),
            PathBuf::from(r"C:\L\screenManual\config.json")
        );
        assert_eq!(p.models_dir(), PathBuf::from(r"C:\L\screenManual\models"));
        assert_eq!(p.logs_dir(), PathBuf::from(r"C:\L\screenManual\logs"));
    }

    #[test]
    #[ignore = "usa o Windows Credential Manager de verdade (restaura o token depois)"]
    fn token_roundtrip_in_credential_manager() {
        let before = load_token().unwrap();
        save_token("  teste-screenmanual  ").unwrap();
        assert_eq!(load_token().unwrap().as_deref(), Some("teste-screenmanual"));
        delete_token().unwrap();
        assert_eq!(load_token().unwrap(), None);
        delete_token().unwrap(); // apagar o que não existe não é erro
        if let Some(t) = before {
            save_token(&t).unwrap();
        }
    }
}
