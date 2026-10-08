use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manual {
    pub schema_version: u32,
    pub titulo: String,
    #[serde(default)]
    pub objetivo: String,
    #[serde(default)]
    pub pre_requisitos: Vec<String>,
    pub secoes: Vec<Secao>,
    #[serde(default)]
    pub descartados: Vec<Descartado>,
    /// Emoji do documento no Outline (spec 2026-10-08 §1.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icone: Option<String>,
    /// Imagens do operador (`crops/<id>.png`), aceitas em `Passo.imagem` (spec 2026-10-08 §3.1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extras: Vec<String>,
}

pub const ICONE_PADRAO: &str = "📘";

impl Manual {
    /// O emoji escolhido pela IA, ou o padrão quando falta ou não parece um emoji.
    pub fn icone_efetivo(&self) -> &str {
        match self.icone.as_deref().map(str::trim) {
            Some(i)
                if (1..=16).contains(&i.len()) && !i.chars().any(|c| c.is_ascii_alphanumeric()) =>
            {
                i
            }
            _ => ICONE_PADRAO,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Secao {
    pub titulo: String,
    pub passos: Vec<Passo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Passo {
    #[serde(default)]
    pub candidatos: Vec<String>,
    pub imagem: Option<String>,
    pub texto: String,
    pub aviso: Option<String>,
    pub dica: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Descartado {
    pub id: String,
    pub motivo: String,
}

/// Cópia de arquivo dentro da pasta da sessão (ex.: crops/c012.png → img/c012.png).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageCopy {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub markdown: String,
    pub images: Vec<ImageCopy>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spec_steps_example() {
        let json = r#"{"schema_version":1,"titulo":"Emitir NFS-e","objetivo":"…","pre_requisitos":["…"],
          "secoes":[{"titulo":"Cadastro do tomador","passos":[
            {"candidatos":["c012"],"imagem":"c012","texto":"Preencha **CNPJ** com o CNPJ do cliente, sem pontuação.","aviso":null,"dica":null}]}],
          "descartados":[{"id":"c007","motivo":"clique acidental, desfeito com Esc"}]}"#;
        let m: Manual = serde_json::from_str(json).unwrap();
        assert_eq!(m.secoes[0].passos[0].imagem.as_deref(), Some("c012"));
        assert_eq!(m.descartados[0].id, "c007");
    }

    #[test]
    fn steps_antigos_leem_sem_icone_nem_extras() {
        let m: Manual =
            serde_json::from_str(r#"{"schema_version":1,"titulo":"T","secoes":[]}"#).unwrap();
        assert_eq!((m.icone.as_deref(), m.extras.len()), (None, 0));
        let s = serde_json::to_string(&m).unwrap();
        assert!(!s.contains("icone") && !s.contains("extras"), "{s}");
    }

    #[test]
    fn icone_efetivo_cai_no_padrao_quando_nao_e_emoji() {
        let mut m: Manual =
            serde_json::from_str(r#"{"schema_version":1,"titulo":"T","secoes":[]}"#).unwrap();
        assert_eq!(m.icone_efetivo(), ICONE_PADRAO);
        m.icone = Some(" 🖨️ ".into());
        assert_eq!(m.icone_efetivo(), "🖨️");
        m.icone = Some("abc".into());
        assert_eq!(m.icone_efetivo(), ICONE_PADRAO);
        m.icone = Some("   ".into());
        assert_eq!(m.icone_efetivo(), ICONE_PADRAO);
        m.icone = Some("🖨️🖨️🖨️🖨️🖨️".into());
        assert_eq!(m.icone_efetivo(), ICONE_PADRAO, "mais de 16 bytes");
    }
}
