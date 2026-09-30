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
}
