use serde::{Deserialize, Serialize};

use super::Rect;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mark {
    Rect { rect: Rect },
    Circle { x: i32, y: i32, r: i32 },
}

/// Coordenadas em pixels de tela; `monitor` é o retângulo que o print `source` cobre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CropSpec {
    pub source: String,
    pub monitor: Rect,
    pub region: Rect,
    pub mark: Mark,
    pub max_side: u32,
    pub out: String,
}
