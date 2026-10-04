use serde::{Deserialize, Serialize};

/// Constantes do build; ficam no config.json para calibrar com sessões reais.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildConfig {
    pub lead_ms: u64,
    pub double_click_ms: u64,
    pub drag_px: i32,
    pub error_window_ms: u64,
    pub no_change_hamming: u32,
    pub crop_pad: i32,
    pub min_crop: (i32, i32),
    pub fixed_crop: (i32, i32),
    pub max_side: u32,
    pub switch_after_click_ms: u64,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            lead_ms: 1500,
            double_click_ms: 500,
            drag_px: 10,
            error_window_ms: 5000,
            no_change_hamming: 5,
            crop_pad: 120,
            min_crop: (480, 300),
            fixed_crop: (640, 400),
            max_side: 1280,
            switch_after_click_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureConfig {
    pub deny_processes: Vec<String>,
    pub deny_title_words: Vec<String>,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            deny_processes: [
                "keepass.exe",
                "keepassxc.exe",
                "bitwarden.exe",
                "1password.exe",
            ]
            .map(String::from)
            .to_vec(),
            deny_title_words: ["senha", "password", "internet banking"]
                .map(String::from)
                .to_vec(),
        }
    }
}

/// Modelo do whisper escolhido nas configurações (spec D11); os nomes de arquivo ficam no adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptionModel {
    Rapido,
    Equilibrado,
    #[default]
    Preciso,
}

/// Bloco `transcricao` do config.json (spec §6.1 item 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TranscribeConfig {
    pub modelo: TranscriptionModel,
    /// Liga o initial_prompt com o vocabulário da sessão (experimental; spike achado 4).
    pub vocabulario: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcribe_config_defaults_and_parses_portuguese_names() {
        let d: TranscribeConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(
            d,
            TranscribeConfig {
                modelo: TranscriptionModel::Preciso,
                vocabulario: false
            }
        );
        let c: TranscribeConfig =
            serde_json::from_str(r#"{"modelo":"rapido","vocabulario":true}"#).unwrap();
        assert_eq!(
            c,
            TranscribeConfig {
                modelo: TranscriptionModel::Rapido,
                vocabulario: true
            }
        );
        assert_eq!(
            serde_json::to_string(&TranscriptionModel::Equilibrado).unwrap(),
            "\"equilibrado\""
        );
    }
}
