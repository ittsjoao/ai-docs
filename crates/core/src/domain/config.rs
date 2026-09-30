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
            deny_processes: ["keepass.exe", "keepassxc.exe", "bitwarden.exe", "1password.exe"]
                .map(String::from)
                .to_vec(),
            deny_title_words: ["senha", "password", "internet banking"].map(String::from).to_vec(),
        }
    }
}
