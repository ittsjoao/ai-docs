//! Adapter `ManualAgent`: roda o Claude Code headless (`claude -p`) na pasta da sessão (spec §7.2).
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use screenmanual_core::ports::{AgentMode, AgentResult, ManualAgent, PortResult};
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
        "100",
        "--strict-mcp-config",
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
    Done {
        is_error: bool,
        subtype: String,
        text: String,
    },
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
            subtype: v["subtype"].as_str().unwrap_or("").to_string(),
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
                return Some(describe_command(cmd));
            }
            Some(match words.next() {
                Some("render") => "montando o manual".to_string(),
                Some("publish") => "publicando o rascunho no Outline".to_string(),
                Some("fetch") => "conferindo o que foi publicado".to_string(),
                Some("redact") => "tarjando um dado sensível".to_string(),
                _ => describe_command(cmd),
            })
        }
        _ => None,
    }
}

/// `comando: ...` com só a primeira linha, em até 80 caracteres (`…` quando cortou).
fn describe_command(cmd: &str) -> String {
    let mut lines = cmd.lines();
    let first = lines.next().unwrap_or("");
    let mut short: String = first.chars().take(80).collect();
    if short.len() < first.len() || lines.next().is_some() {
        short.push('…');
    }
    format!("comando: {short}")
}

const OMITTED_IMAGE: &str = r#"{"type":"user","omitido":"imagem"}"#;

/// Linha do `stream-json` como vai para `logs/`: mensagem `user` com imagem vira um resumo,
/// porque o base64 dos prints levava o log a ~10 MB por geração (validação de 2026-10-04).
pub fn log_line(line: &str) -> &str {
    if line.contains("\"image\"")
        && serde_json::from_str::<Value>(line).is_ok_and(|v| v["type"] == "user")
    {
        return OMITTED_IMAGE;
    }
    line
}

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

/// Tolerância depois que o Claude terminou (evento `result` ou processo encerrado): netos que
/// herdaram o stdout/stderr podem segurar os pipes, então passado isso paramos de esperar o EOF.
const GRACE: Duration = Duration::from_secs(2);

/// Adapter da porta `ManualAgent` (spec §7.2).
pub struct ClaudeAgent {
    /// `claude.exe`/`claude.cmd` (veja `find_claude`).
    pub claude: PathBuf,
    /// Pasta do `screenmanual-cli.exe`; vai na frente do PATH do Claude.
    pub cli_dir: PathBuf,
    pub outline_url: String,
    pub outline_token: String,
    /// `--model`; vazio = padrão da conta.
    pub model: String,
    pub timeout: Duration,
    /// Cancelar da UI; `run` zera ao começar.
    pub cancel: Arc<AtomicBool>,
}

impl ClaudeAgent {
    /// Padrões do spec: 15 min de limite, sem cancelamento pedido.
    pub fn new(
        claude: PathBuf,
        cli_dir: PathBuf,
        outline_url: &str,
        outline_token: &str,
        model: &str,
    ) -> Self {
        Self {
            claude,
            cli_dir,
            outline_url: outline_url.to_string(),
            outline_token: outline_token.to_string(),
            model: model.to_string(),
            timeout: Duration::from_secs(15 * 60),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// O `Read` do Claude Code recusa caminhos 8.3 como `JOAO~1.SIL` (validação de 2026-10-04).
fn long_dir(dir: &Path) -> PathBuf {
    std::fs::canonicalize(dir)
        .map(|p| strip_verbatim(&p))
        .unwrap_or_else(|_| dir.to_path_buf())
}

/// Tira o prefixo verbatim do Windows: `\\?\C:\x` vira `C:\x` e `\\?\UNC\s\x` vira `\\s\x`.
fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

impl ManualAgent for ClaudeAgent {
    fn run(
        &self,
        dir: &Path,
        mode: AgentMode,
        progress: &mut dyn FnMut(&str),
    ) -> PortResult<AgentResult> {
        let dir = long_dir(dir);
        let dir = dir.as_path();
        self.cancel.store(false, Ordering::Relaxed);
        let result_path = dir.join("result.json");
        match std::fs::remove_file(&result_path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(e).context("falha ao apagar o result.json anterior");
            }
            _ => {}
        }
        let logs = dir.join("logs");
        std::fs::create_dir_all(&logs)
            .with_context(|| format!("falha ao criar {}", logs.display()))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let mut log = std::fs::File::create(logs.join(format!("claude-{stamp}.jsonl")))
            .context("falha ao criar o log do Claude")?;

        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let path = std::env::join_paths(
            std::iter::once(self.cli_dir.clone()).chain(std::env::split_paths(&inherited)),
        )
        .context("PATH inválido")?;
        let mut child = no_window(
            Command::new(&self.claude)
                .args(claude_args(mode, &self.model))
                .current_dir(dir)
                .env("PATH", path)
                .env("OUTLINE_URL", &self.outline_url)
                .env("OUTLINE_API_TOKEN", &self.outline_token)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        )
        .spawn()
        .with_context(|| format!("falha ao iniciar o Claude Code ({})", self.claude.display()))?;

        let (tx, rx) = channel::<String>();
        let stdout = child
            .stdout
            .take()
            .context("stdout do Claude indisponível")?;
        std::thread::spawn(move || {
            for line in BufReader::new(stdout)
                .lines()
                .map_while(std::io::Result::ok)
            {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut stderr = child
            .stderr
            .take()
            .context("stderr do Claude indisponível")?;
        let (err_tx, err_rx) = channel::<String>();
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stderr.read_to_string(&mut s);
            let _ = err_tx.send(s);
        });

        let deadline = Instant::now() + self.timeout;
        let (mut done, mut last) = (None, String::new());
        let mut finished_at: Option<Instant> = None;
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(line) => {
                    let _ = writeln!(log, "{}", log_line(&line)); // log é auxiliar: falha ao gravar não derruba a geração
                    for ev in parse_line(&line) {
                        match ev {
                            StreamEvent::Progress(p) if p != last => {
                                progress(&p);
                                last = p;
                            }
                            StreamEvent::Progress(_) => {}
                            d @ StreamEvent::Done { .. } => {
                                done = Some(d);
                                finished_at.get_or_insert_with(Instant::now);
                            }
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if finished_at.is_none() && matches!(child.try_wait(), Ok(Some(_))) {
                finished_at = Some(Instant::now());
            }
            if finished_at.is_some_and(|t| t.elapsed() >= GRACE) {
                break;
            }
            if self.cancel.load(Ordering::Relaxed) {
                kill_tree(&mut child);
                bail!("geração cancelada");
            }
            if Instant::now() >= deadline {
                kill_tree(&mut child);
                bail!(
                    "o Claude Code passou do tempo limite ({} s); geração interrompida",
                    self.timeout.as_secs()
                );
            }
        }
        // sobrou processo (neto segurando o pipe, claude lento para sair): mata a árvore, que também espera
        if !matches!(child.try_wait(), Ok(Some(_))) {
            kill_tree(&mut child);
        }
        let status = match child.try_wait() {
            Ok(Some(s)) => s.to_string(),
            _ => "sem status".to_string(),
        };
        let stderr = err_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_default();

        let (is_error, subtype, text) = match done {
            Some(StreamEvent::Done {
                is_error,
                subtype,
                text,
            }) => (is_error, subtype, Some(text)),
            _ => (false, String::new(), None),
        };
        if (is_error || text.is_none())
            && (is_login_error(text.as_deref().unwrap_or("")) || is_login_error(&stderr))
        {
            return Err(LoginRequired.into());
        }
        let Some(text) = text else {
            bail!(
                "o Claude Code saiu ({status}) sem resultado: {}",
                tail(&stderr)
            );
        };
        if is_error || text.trim().is_empty() {
            bail!("o Claude Code parou ({subtype}): {text}");
        }
        let bytes = match std::fs::read(&result_path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!("a skill terminou sem gravar result.json: {text}");
            }
            Err(e) => return Err(e).context("falha ao ler o result.json"),
        };
        serde_json::from_slice(&bytes).context("result.json inválido")
    }
}

#[cfg(windows)]
fn taskkill_path() -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(root).join("System32").join("taskkill.exe")
}

/// Mata o claude e os filhos (shell, screenmanual-cli); `Child::kill` sozinho deixaria netos vivos.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    let _ = no_window(
        Command::new(taskkill_path())
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .status();
    let _ = child.kill();
    let _ = child.wait();
}

/// Últimos ~500 caracteres (sem cortar um caractere UTF-8 ao meio).
fn tail(s: &str) -> String {
    let chars: Vec<char> = s.trim().chars().collect();
    chars[chars.len().saturating_sub(500)..].iter().collect()
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
            &a[..10],
            [
                "-p",
                "/gerar-manual gerar",
                "--output-format",
                "stream-json",
                "--verbose",
                "--max-turns",
                "100",
                "--strict-mcp-config",
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
        assert_eq!(a.len(), 10 + 1 + 12 + 3);

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
        assert_eq!(
            parse_line(&tool("Bash", r#"{"command":"echo a\necho b"}"#)),
            p("comando: echo a…")
        );
        let long = "x".repeat(100);
        assert_eq!(
            parse_line(&tool("Bash", &format!(r#"{{"command":"{long}"}}"#))),
            p(&format!("comando: {}…", "x".repeat(80)))
        );
        let acentos = "é".repeat(90);
        assert_eq!(
            parse_line(&tool("Bash", &format!(r#"{{"command":"{acentos}"}}"#))),
            p(&format!("comando: {}…", "é".repeat(80)))
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
                subtype: "success".into(),
                text: "Publicado.".into()
            }]
        );
        assert_eq!(
            parse_line(r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#),
            vec![StreamEvent::Done {
                is_error: true,
                subtype: "error_max_turns".into(),
                text: String::new()
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
    fn strip_verbatim_handles_drive_unc_and_plain_paths() {
        let f = |s: &str| strip_verbatim(Path::new(s));
        assert_eq!(f(r"\\?\C:\x\y"), PathBuf::from(r"C:\x\y"));
        assert_eq!(f(r"\\?\UNC\srv\share\x"), PathBuf::from(r"\\srv\share\x"));
        assert_eq!(f(r"C:\x"), PathBuf::from(r"C:\x"));
        assert_eq!(f("rel/x"), PathBuf::from("rel/x"));
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

    #[test]
    fn log_drops_base64_images_from_user_lines() {
        let img = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":[{"type":"image","source":{"type":"base64","data":"iVBORw0KGgo"}}]}]}}"#;
        assert_eq!(log_line(img), r#"{"type":"user","omitido":"imagem"}"#);
        let txt =
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}}"#;
        assert_eq!(log_line(txt), txt);
        let asst = r#"{"type":"assistant","message":{"content":[{"type":"image","source":{}}]}}"#;
        assert_eq!(log_line(asst), asst);
        assert_eq!(log_line("não é json \"image\""), "não é json \"image\"");
    }
}
