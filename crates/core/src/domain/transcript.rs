use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub w: String,
    pub s: u64,
    pub e: u64,
}

/// Tempos em ms na linha do tempo da sessão (já somado o audio_offset_ms).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub text: String,
    #[serde(default)]
    pub words: Vec<Word>,
}
