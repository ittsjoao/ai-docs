//! Adapter `ManualAgent`: roda o Claude Code headless (`claude -p`) na pasta da sessão (spec §7.2).
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};
use screenmanual_core::ports::AgentMode;
use serde_json::Value;

/// Skill embutida; `install_skill` a grava em `~/.claude/skills/gerar-manual/` (spec §8).
pub const SKILL: &str = include_str!("../../../skill/gerar-manual/SKILL.md");

/// Ferramentas liberadas (spec §7.2). `Write(...)` não libera escrita: a regra de arquivo é
/// `Edit` (validação de 2026-10-04).
const ALLOWED_TOOLS: [&str; 12] = [
    "Read(./**)",
    "Glob",
    "Edit(./steps.json)",
    "Edit(./result.json)",
    "PowerShell(screenmanual-cli render:*)",
    "PowerShell(screenmanual-cli publish:*)",
    "PowerShell(screenmanual-cli fetch:*)",
    "PowerShell(screenmanual-cli redact:*)",
    "Bash(screenmanual-cli render:*)",
    "Bash(screenmanual-cli publish:*)",
    "Bash(screenmanual-cli fetch:*)",
    "Bash(screenmanual-cli redact:*)",
];

/// O Claude Code não está logado; a UI oferece "Fazer login" (spec §7.2).
#[derive(Debug)]
pub struct LoginRequired;

impl std::fmt::Display for LoginRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("o Claude Code não está logado: abra um terminal, rode `claude` e faça login")
    }
}

impl std::error::Error for LoginRequired {}

/// Grava a skill quando falta ou mudou (spec §8). Devolve `true` se gravou.
pub fn install_skill(home: &Path) -> Result<bool> {
    let dir = home.join(".claude").join("skills").join("gerar-manual");
    let path = dir.join("SKILL.md");
    if std::fs::read_to_string(&path).ok().as_deref() == Some(SKILL) {
        return Ok(false);
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, SKILL)?;
    Ok(true)
}

/// `claude.exe` ou `claude.cmd` (npm) no PATH; senão o instalador nativo (`~/.local/bin`).
pub fn find_claude(path_var: &OsStr, home: &Path) -> Option<PathBuf> {
    std::env::split_paths(path_var)
        .flat_map(|d| [d.join("claude.exe"), d.join("claude.cmd")])
        .chain([home.join(".local").join("bin").join("claude.exe")])
        .find(|p| p.is_file())
}

/// `claude --version`; falha = Claude Code ausente (spec §7.2).
pub fn check_claude(claude: &Path) -> Result<String> {
    match no_window(Command::new(claude).arg("--version")).output() {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()),
        _ => bail!(
            "Claude Code não instalado ou não encontrado ({}); instale e faça login com `claude`",
            claude.display()
        ),
    }
}

pub fn claude_args(mode: AgentMode, model: &str) -> Vec<String> {
    let prompt = match mode {
        AgentMode::Gerar => "/gerar-manual gerar",
        AgentMode::Melhoria => "/gerar-manual melhoria",
    };
    let mut a: Vec<String> = [
        "-p",
        prompt,
        "--output-format",
        "stream-json",
        "--verbose",
        "--max-turns",
        "40",
    ]
    .map(String::from)
    .to_vec();
    if !model.is_empty() {
        a.extend(["--model".to_string(), model.to_string()]);
    }
    a.push("--allowedTools".into());
    a.extend(ALLOWED_TOOLS.map(String::from));
    a.extend(["--disallowedTools", "WebFetch", "WebSearch"].map(String::from));
    a
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// O que o Claude está fazendo, em português, para a barra de progresso.
    Progress(String),
    /// Última linha do `stream-json` (`type: result`).
    Done { is_error: bool, text: String },
}

/// Uma linha do `--output-format stream-json`; linhas desconhecidas são ignoradas.
pub fn parse_line(line: &str) -> Vec<StreamEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    match v["type"].as_str() {
        Some("assistant") => v["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["type"] == "tool_use")
            .filter_map(|c| describe(c["name"].as_str()?, &c["input"]))
            .map(StreamEvent::Progress)
            .collect(),
        Some("result") => vec![StreamEvent::Done {
            is_error: v["is_error"].as_bool().unwrap_or(false),
            text: v["result"].as_str().unwrap_or("").to_string(),
        }],
        _ => vec![],
    }
}

fn describe(tool: &str, input: &Value) -> Option<String> {
    let file = input["file_path"]
        .as_str()
        .map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p));
    match tool {
        "Read" => file.map(|f| {
            if f.ends_with(".png") {
                "lendo as imagens da gravação".to_string()
            } else {
                format!("lendo {f}")
            }
        }),
        "Write" | "Edit" => file.map(|f| match f {
            "steps.json" => "escrevendo o manual".to_string(),
            "result.json" => "registrando o resultado".to_string(),
            _ => format!("editando {f}"),
        }),
        "Bash" | "PowerShell" => {
            let cmd = input["command"].as_str()?.trim();
            let mut words = cmd.split_whitespace();
            if words.next() != Some("screenmanual-cli") {
                return Some(format!("comando: {cmd}"));
            }
            Some(match words.next() {
                Some("render") => "montando o manual".to_string(),
                Some("publish") => "publicando o rascunho no Outline".to_string(),
                Some("fetch") => "conferindo o que foi publicado".to_string(),
                Some("redact") => "tarjando um dado sensível".to_string(),
                _ => format!("comando: {cmd}"),
            })
        }
        _ => None,
    }
}

#[allow(dead_code)] // usado pelo ClaudeAgent (Task 4)
fn is_login_error(text: &str) -> bool {
    let t = text.to_lowercase();
    [
        "not logged in",
        "/login",
        "invalid api key",
        "oauth token has expired",
        "authentication_error",
    ]
    .iter()
    .any(|k| t.contains(k))
}

/// `CREATE_NO_WINDOW`: o app é GUI; sem isso cada `claude`/`taskkill` abriria um console.
fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("smagentu-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn args_follow_the_spec() {
        let a = claude_args(AgentMode::Gerar, "sonnet");
        assert_eq!(
            &a[..9],
            [
                "-p",
                "/gerar-manual gerar",
                "--output-format",
                "stream-json",
                "--verbose",
                "--max-turns",
                "40",
                "--model",
                "sonnet"
            ]
        );
        let allowed = a.iter().position(|x| x == "--allowedTools").unwrap();
        assert_eq!(
            &a[allowed + 1..allowed + 5],
            [
                "Read(./**)",
                "Glob",
                "Edit(./steps.json)",
                "Edit(./result.json)"
            ]
        );
        assert_eq!(
            &a[a.len() - 3..],
            ["--disallowedTools", "WebFetch", "WebSearch"]
        );
        assert_eq!(a.len(), 9 + 1 + 12 + 3);

        let m = claude_args(AgentMode::Melhoria, "");
        assert_eq!(m[1], "/gerar-manual melhoria");
        assert!(!m.contains(&"--model".to_string()));
    }

    #[test]
    fn stream_lines_become_progress_and_done() {
        let tool = |name: &str, input: &str| {
            format!(
                r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"{name}","input":{input}}}]}}}}"#
            )
        };
        let p = |s: &str| vec![StreamEvent::Progress(s.into())];
        assert_eq!(
            parse_line(&tool("Read", r#"{"file_path":"C:\\s\\crops\\c001.png"}"#)),
            p("lendo as imagens da gravação")
        );
        assert_eq!(
            parse_line(&tool("Read", r#"{"file_path":"C:\\s\\candidates.json"}"#)),
            p("lendo candidates.json")
        );
        assert_eq!(
            parse_line(&tool("Write", r#"{"file_path":"C:/s/steps.json"}"#)),
            p("escrevendo o manual")
        );
        assert_eq!(
            parse_line(&tool("Edit", r#"{"file_path":"result.json"}"#)),
            p("registrando o resultado")
        );
        assert_eq!(
            parse_line(&tool(
                "PowerShell",
                r#"{"command":"screenmanual-cli redact crops/c005.png 1,2,3,4"}"#
            )),
            p("tarjando um dado sensível")
        );
        assert_eq!(
            parse_line(&tool("Bash", r#"{"command":"screenmanual-cli publish"}"#)),
            p("publicando o rascunho no Outline")
        );
        assert_eq!(
            parse_line(&tool("Bash", r#"{"command":"screenmanual-cli render"}"#)),
            p("montando o manual")
        );
        assert_eq!(
            parse_line(&tool("Bash", r#"{"command":"screenmanual-cli fetch"}"#)),
            p("conferindo o que foi publicado")
        );
        assert_eq!(
            parse_line(&tool("Bash", r#"{"command":"dir"}"#)),
            p("comando: dir")
        );
        assert!(parse_line(&tool("Glob", r#"{"pattern":"crops/*"}"#)).is_empty());
        assert!(parse_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"oi"}]}}"#
        )
        .is_empty());
        assert!(parse_line(r#"{"type":"system","subtype":"init"}"#).is_empty());
        assert!(parse_line("não é json").is_empty());
        assert_eq!(
            parse_line(
                r#"{"type":"result","subtype":"success","is_error":false,"result":"Publicado.","total_cost_usd":0.6}"#
            ),
            vec![StreamEvent::Done {
                is_error: false,
                text: "Publicado.".into()
            }]
        );
    }

    #[test]
    fn login_errors_are_recognized() {
        assert!(is_login_error("Not logged in · Please run /login"));
        assert!(is_login_error("Invalid API key · Fix external API key"));
        assert!(!is_login_error("Publicado como rascunho."));
    }

    #[test]
    fn claude_is_found_in_path_then_in_the_native_install_dir() {
        let (a, b, home) = (temp("a"), temp("b"), temp("home"));
        std::fs::write(b.join("claude.cmd"), "").unwrap();
        let path = std::env::join_paths([&a, &b]).unwrap();
        assert_eq!(find_claude(&path, &home), Some(b.join("claude.cmd")));
        assert_eq!(
            find_claude(&std::env::join_paths([&a]).unwrap(), &home),
            None
        );
        let native = home.join(".local").join("bin");
        std::fs::create_dir_all(&native).unwrap();
        std::fs::write(native.join("claude.exe"), "").unwrap();
        assert_eq!(
            find_claude(&std::env::join_paths([&a]).unwrap(), &home),
            Some(native.join("claude.exe"))
        );
    }

    #[test]
    fn skill_is_installed_only_when_it_changes() {
        assert!(SKILL.contains("name: gerar-manual"));
        let home = temp("skill");
        assert!(install_skill(&home).unwrap());
        let path = home
            .join(".claude")
            .join("skills")
            .join("gerar-manual")
            .join("SKILL.md");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SKILL);
        assert!(!install_skill(&home).unwrap());
        std::fs::write(&path, "versão antiga").unwrap();
        assert!(install_skill(&home).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SKILL);
    }
}
