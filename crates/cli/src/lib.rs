//! `screenmanual-cli`: comandos que a skill /gerar-manual chama dentro da pasta da sessão (spec §7.3).
//! Toda a saída vai para o stdout (a skill não lê stderr nem pode redirecionar; spike §5).
use std::path::Path;

use screenmanual_core::commands::{fetch_published, publish_draft, CommandError};
use screenmanual_core::domain::PublishStatus;
use screenmanual_core::pipeline::render::render;
use screenmanual_core::ports::{SessionStore, Wiki};
use screenmanual_store::{redact, session_path, FsStore};
use serde_json::json;

const USAGE: &str =
    "uso: screenmanual-cli render | publish | fetch | redact <crops/arquivo.png> <x,y,w,h>";
/// Margem extra da tarja, em pixels (spec §7.3).
const REDACT_MARGIN: u32 = 6;

pub fn run<W: Wiki>(
    args: &[String],
    dir: &Path,
    wiki: impl FnOnce() -> anyhow::Result<W>,
) -> (i32, String) {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let known = matches!(
        args.as_slice(),
        ["render"] | ["publish"] | ["fetch"] | ["redact", _, _]
    );
    if !known {
        return (2, USAGE.to_string());
    }
    let (Some(root), Some(id)) = (dir.parent(), dir.file_name().and_then(|n| n.to_str())) else {
        return (
            1,
            "erro: rode o screenmanual-cli dentro da pasta da sessão".into(),
        );
    };
    if !dir.join("session.json").is_file() {
        return (
            1,
            "erro: rode o screenmanual-cli dentro da pasta da sessão (session.json não encontrado)"
                .into(),
        );
    }
    let store = FsStore::new(root);
    match args.as_slice() {
        ["render"] => report(render_cmd(&store, id)),
        ["redact", crop, rect] => report(redact_cmd(dir, crop, rect)),
        ["publish"] => match wiki()
            .map_err(CommandError::from)
            .and_then(|w| publish_draft(&store, &w, id))
        {
            Ok(state) => {
                let status = match state.status {
                    Some(PublishStatus::Published) => "published",
                    _ => "draft",
                };
                (
                    0,
                    json!({ "url": state.url, "revision": state.revision, "status": status })
                        .to_string(),
                )
            }
            Err(e @ CommandError::EditedManually { .. }) => (3, format!("erro: {e}")),
            Err(e) => (1, format!("erro: {e:#}")),
        },
        ["fetch"] => match wiki()
            .map_err(CommandError::from)
            .and_then(|w| fetch_published(&store, &w, id))
        {
            Ok(r) => {
                let imagens = std::fs::read_dir(dir.join("published/img"))
                    .map(|d| d.count())
                    .unwrap_or(0);
                (
                    0,
                    json!({ "arquivo": "published/manual.md", "imagens": imagens, "faltando": r.missing, "divergentes": r.mismatched })
                        .to_string(),
                )
            }
            Err(e) => (1, format!("erro: {e:#}")),
        },
        _ => (2, USAGE.to_string()),
    }
}

fn report(result: anyhow::Result<String>) -> (i32, String) {
    match result {
        Ok(msg) => (0, msg),
        Err(e) => (1, format!("erro: {e:#}")),
    }
}

fn render_cmd(store: &FsStore, id: &str) -> anyhow::Result<String> {
    let manual = store.manual(id)?.ok_or_else(|| {
        anyhow::anyhow!("steps.json não encontrado; escreva o steps.json primeiro")
    })?;
    let candidates = store.candidates(id)?;
    let rendered = render(&manual, &candidates, id)?;
    store.save_rendered(id, &rendered)?;
    Ok(format!(
        "ok: manual.md gerado com {} imagens em img/",
        rendered.images.len()
    ))
}

fn redact_cmd(dir: &Path, crop: &str, rect: &str) -> anyhow::Result<String> {
    if !crop.starts_with("crops/") {
        anyhow::bail!("redact só tarja arquivos em crops/ (o render copia para img/): {crop}");
    }
    let path = session_path(dir, crop)?;
    let n: Vec<u32> = rect
        .split(',')
        .map(|v| v.trim().parse::<u32>())
        .collect::<Result<_, _>>()
        .map_err(|_| {
            anyhow::anyhow!("retângulo inválido: {rect}; use x,y,w,h em pixels do recorte")
        })?;
    let [x, y, w, h] = n.as_slice() else {
        anyhow::bail!("retângulo inválido: {rect}; use x,y,w,h em pixels do recorte");
    };
    redact(&path, *x, *y, *w, *h, REDACT_MARGIN)?;
    Ok(format!("ok: {crop} tarjado em {x},{y},{w},{h} (+{REDACT_MARGIN} px de margem); rode render e publish de novo"))
}
