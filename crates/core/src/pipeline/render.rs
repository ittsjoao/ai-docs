use std::collections::HashMap;

use crate::domain::{Candidate, ImageCopy, Manual, Rendered};

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum RenderError {
    #[error("schema_version {0} não suportado em steps.json; use 1")]
    SchemaVersion(u32),
    #[error("passo {passo}: texto vazio")]
    EmptyText { passo: usize },
    #[error("passo {passo}: candidato '{id}' não existe em candidates.json")]
    UnknownCandidate { passo: usize, id: String },
    #[error("passo {passo}: candidato '{id}' não tem crop; escolha outro candidato para imagem ou use null")]
    ImageWithoutCrop { passo: usize, id: String },
}

pub fn render(
    manual: &Manual,
    candidates: &[Candidate],
    session_id: &str,
) -> Result<Rendered, RenderError> {
    if manual.schema_version != 1 {
        return Err(RenderError::SchemaVersion(manual.schema_version));
    }
    let by_id: HashMap<&str, &Candidate> = candidates.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut md = format!(
        "---\nsessao: {session_id}\n---\n\n# {}\n\n",
        manual.titulo.trim()
    );
    if !manual.objetivo.trim().is_empty() {
        md += &format!("{}\n\n", manual.objetivo.trim());
    }
    if !manual.pre_requisitos.is_empty() {
        md += "## Pré-requisitos\n\n";
        for p in &manual.pre_requisitos {
            md += &format!("- {}\n", p.trim());
        }
        md += "\n";
    }
    let mut n = 0;
    let mut images: Vec<ImageCopy> = Vec::new();
    for secao in &manual.secoes {
        md += &format!("## {}\n\n", secao.titulo.trim());
        for passo in &secao.passos {
            n += 1;
            if passo.texto.trim().is_empty() {
                return Err(RenderError::EmptyText { passo: n });
            }
            if let Some(id) = passo
                .candidatos
                .iter()
                .find(|id| !by_id.contains_key(id.as_str()))
            {
                return Err(RenderError::UnknownCandidate {
                    passo: n,
                    id: id.clone(),
                });
            }
            md += &format!("**Passo {n}.** {}\n\n", passo.texto.trim());
            if let Some(img) = &passo.imagem {
                let cand =
                    by_id
                        .get(img.as_str())
                        .ok_or_else(|| RenderError::UnknownCandidate {
                            passo: n,
                            id: img.clone(),
                        })?;
                let crop = cand
                    .crop
                    .as_ref()
                    .ok_or_else(|| RenderError::ImageWithoutCrop {
                        passo: n,
                        id: img.clone(),
                    })?;
                let to = format!("img/{img}.png");
                md += &format!("![Passo {n}]({to})\n\n");
                if !images.iter().any(|i| i.to == to) {
                    images.push(ImageCopy {
                        from: crop.clone(),
                        to,
                    });
                }
            }
            if let Some(aviso) = passo
                .aviso
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                md += &format!(":::warning\n{aviso}\n:::\n\n");
            }
            if let Some(dica) = passo
                .dica
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                md += &format!(":::tip\n{dica}\n:::\n\n");
            }
        }
    }
    Ok(Rendered {
        markdown: format!("{}\n", md.trim_end()),
        images,
    })
}

#[cfg(test)]
mod tests {
    use crate::domain::*;
    use crate::pipeline::render::*;

    fn cand(id: &str, crop: Option<&str>) -> Candidate {
        Candidate {
            id: id.into(),
            t: 0,
            kind: CandidateKind::Click,
            app: String::new(),
            window: String::new(),
            url: None,
            el: None,
            input: None,
            keys: vec![],
            speech: vec![],
            flags: vec![],
            crop: crop.map(str::to_string),
            context_shot: None,
        }
    }

    fn manual(passo: Passo) -> Manual {
        Manual {
            schema_version: 1,
            titulo: "Emitir NFS-e".into(),
            objetivo: "Emitir nota.".into(),
            pre_requisitos: vec!["Acesso ao ERP".into()],
            secoes: vec![Secao {
                titulo: "Cadastro".into(),
                passos: vec![passo],
            }],
            descartados: vec![],
            icone: None,
            extras: vec![],
        }
    }

    fn passo(img: Option<&str>) -> Passo {
        Passo {
            candidatos: vec!["c001".into()],
            imagem: img.map(str::to_string),
            texto: "Clique em **Nova nota**.".into(),
            aviso: Some("Confira o CNPJ.".into()),
            dica: None,
        }
    }

    #[test]
    fn renders_outline_markdown() {
        let r = render(
            &manual(passo(Some("c001"))),
            &[cand("c001", Some("crops/c001.png"))],
            "s1",
        )
        .unwrap();
        let expected = "---\nsessao: s1\n---\n\n# Emitir NFS-e\n\nEmitir nota.\n\n## Pré-requisitos\n\n- Acesso ao ERP\n\n## Cadastro\n\n**Passo 1.** Clique em **Nova nota**.\n\n![Passo 1](img/c001.png)\n\n:::warning\nConfira o CNPJ.\n:::\n";
        assert_eq!(r.markdown, expected);
        assert_eq!(
            r.images,
            vec![ImageCopy {
                from: "crops/c001.png".into(),
                to: "img/c001.png".into()
            }]
        );
    }

    #[test]
    fn blank_callouts_are_skipped() {
        let mut p = passo(None);
        p.aviso = Some("  \n".into());
        p.dica = Some("".into());
        let r = render(&manual(p), &[cand("c001", None)], "s1").unwrap();
        assert!(!r.markdown.contains(":::"), "{}", r.markdown);
    }

    #[test]
    fn reports_actionable_errors() {
        let cands = [cand("c001", None)];
        assert_eq!(
            render(&manual(passo(Some("c001"))), &cands, "s1"),
            Err(RenderError::ImageWithoutCrop {
                passo: 1,
                id: "c001".into()
            })
        );
        let mut p = passo(None);
        p.candidatos = vec!["c999".into()];
        assert_eq!(
            render(&manual(p), &cands, "s1"),
            Err(RenderError::UnknownCandidate {
                passo: 1,
                id: "c999".into()
            })
        );
        let mut p = passo(None);
        p.texto = " ".into();
        assert_eq!(
            render(&manual(p), &cands, "s1"),
            Err(RenderError::EmptyText { passo: 1 })
        );
        let mut m = manual(passo(None));
        m.schema_version = 2;
        assert_eq!(render(&m, &cands, "s1"), Err(RenderError::SchemaVersion(2)));
    }
}
