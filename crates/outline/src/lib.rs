//! Adapter do Outline: implementa `screenmanual_core::ports::Wiki` (spec §7.3).
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::{multipart, Client};
use screenmanual_core::ports::{Collection, DocInfo, PortResult, Wiki};
use serde_json::{json, Value};

pub struct Outline {
    base: String,
    token: String,
    http: Client,
}

/// URLs do Outline podem vir relativas (storage local) ou absolutas (S3 pré-assinado).
pub(crate) fn absolute(base: &str, url: &str) -> String {
    if url.starts_with('/') {
        format!("{base}{url}")
    } else {
        url.to_string()
    }
}

/// Diz se a URL pertence ao próprio servidor (mesma origem), sem aceitar prefixos como `wiki.x.evil.com`.
pub(crate) fn same_server(base: &str, url: &str) -> bool {
    url == base
        || url
            .strip_prefix(base)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub(crate) fn doc_info(base: &str, data: &Value) -> Result<DocInfo> {
    Ok(DocInfo {
        id: data["id"]
            .as_str()
            .context("resposta do Outline sem id do documento")?
            .to_string(),
        url: absolute(base, data["url"].as_str().unwrap_or("")),
        revision: data["revision"].as_u64().unwrap_or(0),
        text: data["text"].as_str().unwrap_or("").to_string(),
    })
}

pub(crate) fn parse_collections(data: &Value) -> Vec<Collection> {
    data.as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some(Collection {
                id: c["id"].as_str()?.to_string(),
                name: c["name"].as_str()?.to_string(),
            })
        })
        .collect()
}

/// Origem (`https://host:porta`) de uma URL, sem caminho nem query (que podem ter assinatura).
pub(crate) fn host_of(url: &str) -> &str {
    let after = url.find("://").map_or(0, |i| i + 3);
    let end = url[after..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |i| after + i);
    &url[..end]
}

/// `<o que> falhou (<status>): <até 300 caracteres do corpo>`.
pub(crate) fn http_failure(what: &str, status: u16, body: &str) -> String {
    let corpo: String = body.trim().chars().take(300).collect();
    format!("{what} falhou ({status}): {corpo}")
}

pub(crate) fn api_error(method: &str, status: u16, body: &Value) -> String {
    if status == 401 {
        return "token do Outline inválido ou expirado (OUTLINE_API_TOKEN)".to_string();
    }
    let msg = body["message"]
        .as_str()
        .or_else(|| body["error"].as_str())
        .unwrap_or("sem detalhes");
    format!("Outline {method} falhou ({status}): {msg}")
}

impl Outline {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;
        Ok(Self {
            base: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            http,
        })
    }

    /// Lê `OUTLINE_URL` e `OUTLINE_API_TOKEN` (o app e a skill passam por ambiente; spec §7.2).
    pub fn from_env() -> Result<Self> {
        let base = std::env::var("OUTLINE_URL")
            .map_err(|_| anyhow::anyhow!("OUTLINE_URL não definida"))?;
        let token = std::env::var("OUTLINE_API_TOKEN")
            .map_err(|_| anyhow::anyhow!("OUTLINE_API_TOKEN não definido"))?;
        Self::new(&base, &token)
    }

    fn post(&self, method: &str, body: Value) -> Result<Value> {
        let resp = self
            .http
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .with_context(|| format!("falha ao acessar o Outline em {}", self.base))?;
        let status = resp.status();
        let json: Value = resp.json().unwrap_or(Value::Null);
        if !status.is_success() {
            bail!("{}", api_error(method, status.as_u16(), &json));
        }
        Ok(json["data"].clone())
    }
}

impl Wiki for Outline {
    fn collections(&self) -> PortResult<Vec<Collection>> {
        Ok(parse_collections(
            &self.post("collections.list", json!({ "limit": 100 }))?,
        ))
    }

    fn upload_image(&self, doc_id: Option<&str>, name: &str, bytes: &[u8]) -> PortResult<String> {
        let mut body = json!({ "name": name, "contentType": "image/png", "size": bytes.len() });
        if let Some(d) = doc_id {
            body["documentId"] = json!(d);
        }
        let data = self.post("attachments.create", body)?;
        let upload_url = absolute(
            &self.base,
            data["uploadUrl"]
                .as_str()
                .context("Outline não devolveu uploadUrl")?,
        );
        let mut form = multipart::Form::new();
        if let Some(fields) = data["form"].as_object() {
            for (k, v) in fields {
                form = form.text(
                    k.clone(),
                    v.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string()),
                );
            }
        }
        form = form.part(
            "file",
            multipart::Part::bytes(bytes.to_vec())
                .file_name(name.to_string())
                .mime_str("image/png")?,
        );
        let mut req = self.http.post(&upload_url).multipart(form);
        if same_server(&self.base, &upload_url) {
            req = req.bearer_auth(&self.token); // storage local: mesmo servidor; S3 pré-assinado nunca recebe o token
        }
        let resp = req
            .send()
            .with_context(|| format!("falha ao enviar a imagem {name} ao Outline"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let corpo = resp.text().unwrap_or_default();
            bail!(
                "{}",
                http_failure(
                    &format!("upload da imagem {name} para {}", host_of(&upload_url)),
                    status.as_u16(),
                    &corpo
                )
            );
        }
        Ok(data["attachment"]["id"]
            .as_str()
            .context("Outline não devolveu o id do anexo")?
            .to_string())
    }

    fn create_draft(&self, collection_id: &str, title: &str, text: &str) -> PortResult<DocInfo> {
        let data = self.post(
            "documents.create",
            json!({ "collectionId": collection_id, "title": title, "text": text, "publish": false }),
        )?;
        doc_info(&self.base, &data)
    }

    fn update(&self, id: &str, title: &str, text: &str) -> PortResult<DocInfo> {
        doc_info(
            &self.base,
            &self.post(
                "documents.update",
                json!({ "id": id, "title": title, "text": text }),
            )?,
        )
    }

    fn info(&self, id: &str) -> PortResult<DocInfo> {
        doc_info(
            &self.base,
            &self.post("documents.info", json!({ "id": id }))?,
        )
    }

    fn publish(&self, id: &str) -> PortResult<DocInfo> {
        doc_info(
            &self.base,
            &self.post("documents.update", json!({ "id": id, "publish": true }))?,
        )
    }

    fn download_attachment(&self, attachment_id: &str) -> PortResult<Vec<u8>> {
        let resp = self
            .http
            .get(format!(
                "{}/api/attachments.redirect?id={attachment_id}",
                self.base
            ))
            .bearer_auth(&self.token)
            .send()
            .with_context(|| format!("falha ao baixar o anexo {attachment_id}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let corpo = resp.text().unwrap_or_default();
            bail!(
                "{}",
                http_failure(
                    &format!("download do anexo {attachment_id}"),
                    status.as_u16(),
                    &corpo
                )
            );
        }
        Ok(resp.bytes()?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn relative_urls_get_the_base_and_absolute_ones_stay() {
        assert_eq!(
            absolute("https://wiki.x", "/doc/a-1"),
            "https://wiki.x/doc/a-1"
        );
        assert_eq!(
            absolute("https://wiki.x", "https://s3.aws/up?sig=1"),
            "https://s3.aws/up?sig=1"
        );
    }

    #[test]
    fn same_server_is_not_a_string_prefix() {
        assert!(same_server(
            "https://wiki.x",
            "https://wiki.x/api/files.create"
        ));
        assert!(same_server("https://wiki.x", "https://wiki.x"));
        assert!(!same_server("https://wiki.x", "https://wiki.x.evil.com/up"));
        assert!(!same_server("https://wiki.x", "https://wiki.xyz/up"));
        assert!(!same_server("https://wiki.x", "https://s3.aws/up?sig=1"));
    }

    #[test]
    fn doc_info_parses_outline_documents() {
        let data = json!({"id": "d1", "url": "/doc/manual-abc", "revision": 4, "text": "corpo"});
        assert_eq!(
            doc_info("https://wiki.x", &data).unwrap(),
            DocInfo {
                id: "d1".into(),
                url: "https://wiki.x/doc/manual-abc".into(),
                revision: 4,
                text: "corpo".into()
            }
        );
        assert!(doc_info("https://wiki.x", &json!({}))
            .unwrap_err()
            .to_string()
            .contains("id do documento"));
    }

    #[test]
    fn collections_are_parsed_and_incomplete_entries_skipped() {
        let data = json!([{"id": "c1", "name": "TI"}, {"id": "c2"}, {"name": "sem id"}]);
        assert_eq!(
            parse_collections(&data),
            vec![Collection {
                id: "c1".into(),
                name: "TI".into()
            }]
        );
    }

    #[test]
    fn api_errors_are_in_portuguese() {
        assert!(api_error("documents.info", 401, &json!({})).contains("token do Outline inválido"));
        assert_eq!(
            api_error(
                "documents.create",
                400,
                &json!({"message": "collectionId required"})
            ),
            "Outline documents.create falhou (400): collectionId required"
        );
        assert!(api_error("x", 500, &serde_json::Value::Null).contains("sem detalhes"));
    }

    #[test]
    fn http_failures_carry_a_truncated_body() {
        let longo = "é".repeat(500);
        let msg = http_failure("download do anexo a1", 403, &longo);
        assert_eq!(
            msg,
            format!("download do anexo a1 falhou (403): {}", "é".repeat(300))
        );
        assert_eq!(
            http_failure(
                "x",
                500,
                " AccessDenied 
"
            ),
            "x falhou (500): AccessDenied"
        );
        assert_eq!(
            host_of("https://s3.x.com:9000/bucket/k?X-Amz-Signature=abc"),
            "https://s3.x.com:9000"
        );
        assert_eq!(host_of("https://wiki.x"), "https://wiki.x");
    }

    #[test]
    fn new_trims_the_trailing_slash() {
        assert_eq!(
            Outline::new("https://wiki.x/", "t").unwrap().base,
            "https://wiki.x"
        );
    }
}
