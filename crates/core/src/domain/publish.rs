use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PublishStatus {
    Draft,
    Published,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishState {
    pub collection_id: String,
    pub outline_id: Option<String>,
    pub url: Option<String>,
    pub revision: Option<u64>,
    pub status: Option<PublishStatus>,
    /// sha256 da imagem → id do anexo no Outline
    #[serde(default)]
    pub images: BTreeMap<String, String>,
}

impl PublishState {
    pub fn new(collection_id: &str) -> Self {
        Self {
            collection_id: collection_id.to_string(),
            outline_id: None,
            url: None,
            revision: None,
            status: None,
            images: BTreeMap::new(),
        }
    }
}
