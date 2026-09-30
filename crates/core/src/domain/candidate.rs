use serde::{Deserialize, Serialize};

use super::Element;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Click,
    DoubleClick,
    Fill,
    Drag,
    Key,
    Switch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flag {
    Important,
    NoChange,
    PossibleError,
    Noise,
    AfterAutoPause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub chars: u32,
    pub password: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub t: u64,
    pub kind: CandidateKind,
    pub app: String,
    pub window: String,
    pub url: Option<String>,
    pub el: Option<Element>,
    pub input: Option<Input>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub speech: Vec<String>,
    #[serde(default)]
    pub flags: Vec<Flag>,
    pub crop: Option<String>,
    pub context_shot: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spec_candidate_example() {
        let json = r#"{"id":"c012","t":52100,"kind":"fill","app":"erp.exe","window":"Cadastro de Clientes",
          "url":null,"el":{"name":"CNPJ","role":"Edit","quality":"uia","ancestors":["Dados","Cadastro"]},
          "input":{"chars":14,"password":false},"keys":["Enter"],
          "speech":["aqui coloca o CNPJ sem pontuação"],"flags":[],
          "crop":"crops/c012.png","context_shot":null}"#;
        let c: Candidate = serde_json::from_str(json).unwrap();
        assert_eq!(c.kind, CandidateKind::Fill);
        assert_eq!(c.input, Some(Input { chars: 14, password: false }));
        assert_eq!(c.el.unwrap().rect, None);
    }
}
