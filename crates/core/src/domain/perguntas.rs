//! Dúvidas da IA antes de escrever o manual (spec 2026-10-08 §4).
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pergunta {
    pub id: String,
    pub pergunta: String,
    pub opcoes: Vec<String>,
    #[serde(default)]
    pub multipla: bool,
}

/// `perguntas.json`: escrito pela skill; `session_id` e `modo` são acrescentados pelo agente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Perguntas {
    pub perguntas: Vec<Pergunta>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub modo: Option<String>,
}

impl Perguntas {
    pub fn validar(&self) -> Result<(), String> {
        if !(1..=4).contains(&self.perguntas.len()) {
            return Err("de 1 a 4 perguntas".into());
        }
        let mut ids = HashSet::new();
        for p in &self.perguntas {
            if p.id.trim().is_empty() || !ids.insert(p.id.as_str()) {
                return Err(format!("id de pergunta vazio ou repetido: {:?}", p.id));
            }
            if !(2..=4).contains(&p.opcoes.len()) {
                return Err(format!("{}: de 2 a 4 opções", p.id));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resposta {
    pub id: String,
    #[serde(default)]
    pub escolhas: Vec<String>,
    #[serde(default)]
    pub outro: Option<String>,
}

/// `respostas.json`, escrito pelo app.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Respostas {
    #[serde(default)]
    pub pular: bool,
    #[serde(default)]
    pub respostas: Vec<Resposta>,
}

impl Respostas {
    pub fn validar(&self, p: &Perguntas) -> Result<(), String> {
        if self.pular {
            return Ok(());
        }
        for q in &p.perguntas {
            let ok = self.respostas.iter().any(|r| {
                r.id == q.id
                    && (!r.escolhas.is_empty()
                        || r.outro.as_deref().is_some_and(|o| !o.trim().is_empty()))
            });
            if !ok {
                return Err(format!("responda: {}", q.pergunta));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(json: &str) -> Perguntas {
        serde_json::from_str(json).unwrap()
    }

    const UMA: &str =
        r#"{"perguntas":[{"id":"q1","pergunta":"Qual sistema?","opcoes":["ERP","CRM"]}]}"#;

    #[test]
    fn perguntas_validas_e_invalidas() {
        assert_eq!(p(UMA).validar(), Ok(()));
        assert!(p(r#"{"perguntas":[]}"#).validar().is_err());
        assert!(
            p(r#"{"perguntas":[{"id":"q1","pergunta":"?","opcoes":["só uma"]}]}"#)
                .validar()
                .is_err()
        );
        assert!(p(r#"{"perguntas":[{"id":"q1","pergunta":"?","opcoes":["a","b"]},{"id":"q1","pergunta":"?","opcoes":["a","b"]}]}"#).validar().is_err(), "id repetido");
        let cinco = format!(
            r#"{{"perguntas":[{}]}}"#,
            (1..=5)
                .map(|i| format!(r#"{{"id":"q{i}","pergunta":"?","opcoes":["a","b"]}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(p(&cinco).validar().is_err());
    }

    #[test]
    fn respostas_cobrem_toda_pergunta_ou_pulam() {
        let ps = p(UMA);
        let r: Respostas =
            serde_json::from_str(r#"{"respostas":[{"id":"q1","escolhas":["ERP"]}]}"#).unwrap();
        assert_eq!(r.validar(&ps), Ok(()));
        let outro: Respostas =
            serde_json::from_str(r#"{"respostas":[{"id":"q1","escolhas":[],"outro":"Protheus"}]}"#)
                .unwrap();
        assert_eq!(outro.validar(&ps), Ok(()));
        let vazia: Respostas =
            serde_json::from_str(r#"{"respostas":[{"id":"q1","escolhas":[],"outro":"  "}]}"#)
                .unwrap();
        assert!(vazia.validar(&ps).is_err());
        let faltando: Respostas = serde_json::from_str(r#"{"respostas":[]}"#).unwrap();
        assert!(faltando.validar(&ps).is_err());
        let pular: Respostas = serde_json::from_str(r#"{"pular":true}"#).unwrap();
        assert_eq!(pular.validar(&ps), Ok(()));
    }
}
