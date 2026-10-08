use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use screenmanual_agent::{check_claude, ClaudeAgent, LoginRequired};
use screenmanual_core::ports::{AgentMode, AgentOutcome, AgentResult, ManualAgent};

const FAKE: &str = env!("CARGO_BIN_EXE_fake_claude");
const STALE: &str = r#"{"url":"velho","revision":1,"rodadas":0}"#;

fn session(name: &str, mode: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("smagent-{name}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("fake-mode.txt"), mode).unwrap();
    dir
}

fn agent() -> ClaudeAgent {
    ClaudeAgent::new(
        PathBuf::from(FAKE),
        PathBuf::from(r"C:\pasta-do-app"),
        "https://wiki.x",
        "tok-123",
        "sonnet",
    )
}

fn pronto(o: anyhow::Result<AgentOutcome>) -> AgentResult {
    let AgentOutcome::Pronto(r) = o.unwrap() else {
        panic!("esperava Pronto")
    };
    r
}

fn run(
    agent: &ClaudeAgent,
    dir: &Path,
    mode: AgentMode,
) -> (anyhow::Result<AgentOutcome>, Vec<String>) {
    let mut seen = vec![];
    let result = agent.run(dir, mode, &mut |p| seen.push(p.to_string()));
    (result, seen)
}

#[test]
fn gerar_runs_claude_in_the_session_and_reads_result_json() {
    let dir = session("ok", "ok");
    std::fs::write(dir.join("result.json"), STALE).unwrap();
    let (result, seen) = run(&agent(), &dir, AgentMode::Gerar);
    let result = pronto(result);
    assert_eq!(result.url, "https://wiki.x/doc/a");
    assert_eq!(result.revision, 2);
    assert_eq!(
        seen,
        [
            "lendo as imagens da gravação",
            "publicando o rascunho no Outline"
        ],
        "repetições seguidas viram uma"
    );

    let fake: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("fake-args.json")).unwrap())
            .unwrap();
    let args: Vec<&str> = fake["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(&args[..2], ["-p", "/gerar-manual gerar"]);
    assert!(args.contains(&"Edit(./result.json)") && args.contains(&"sonnet"));
    assert_eq!(fake["outline_url"], "https://wiki.x");
    assert_eq!(fake["token"], "tok-123");
    assert!(
        fake["path"]
            .as_str()
            .unwrap()
            .starts_with(r"C:\pasta-do-app;"),
        "{}",
        fake["path"]
    );

    let cwd = fake["cwd"].as_str().unwrap();
    assert!(!cwd.starts_with(r"\\?\"), "{cwd}");
    let long = std::fs::canonicalize(&dir).unwrap();
    let long = long.to_string_lossy();
    let long = long.strip_prefix(r"\\?\").unwrap_or(&long);
    if !long.contains('~') {
        assert!(!cwd.contains('~'), "{cwd}");
    }
    assert_eq!(cwd, long);

    let logs: Vec<_> = std::fs::read_dir(dir.join("logs"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(logs.len(), 1);
    assert!(std::fs::read_to_string(&logs[0])
        .unwrap()
        .contains(r#""type":"result""#));
}

#[test]
fn missing_result_json_is_an_error_and_the_stale_one_is_gone() {
    let dir = session("noresult", "noresult");
    std::fs::write(dir.join("result.json"), STALE).unwrap();
    let err = run(&agent(), &dir, AgentMode::Melhoria)
        .0
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("sem gravar result.json") && err.contains("OUTLINE_API_TOKEN inválido"),
        "{err}"
    );
    assert!(
        !dir.join("result.json").exists(),
        "o result.json antigo não pode passar por resultado novo"
    );
}

#[test]
fn login_errors_are_typed_for_the_ui() {
    let err = run(&agent(), &session("login", "login"), AgentMode::Gerar)
        .0
        .unwrap_err();
    assert!(err.downcast_ref::<LoginRequired>().is_some(), "{err}");
}

#[test]
fn timeout_kills_claude() {
    let mut a = agent();
    a.timeout = Duration::from_secs(2);
    let started = Instant::now();
    let err = run(&a, &session("timeout", "sleep"), AgentMode::Gerar)
        .0
        .unwrap_err()
        .to_string();
    assert!(err.contains("tempo limite"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn cancel_set_before_run_cancels_without_starting_claude() {
    let a = agent();
    a.cancel.store(true, Ordering::Relaxed);
    let dir = session("precancel", "sleep");
    std::fs::write(dir.join("result.json"), STALE).unwrap();
    let started = Instant::now();
    let err = run(&a, &dir, AgentMode::Gerar).0.unwrap_err().to_string();
    assert!(err.contains("cancelada"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(
        !dir.join("logs").exists(),
        "claude não deve ter sido iniciado"
    );
    assert!(dir.join("result.json").exists(), "nada foi apagado");
}

#[test]
fn cancel_kills_claude() {
    let a = agent();
    let cancel = a.cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(1));
        cancel.store(true, Ordering::Relaxed);
    });
    let started = Instant::now();
    let err = run(&a, &session("cancel", "sleep"), AgentMode::Gerar)
        .0
        .unwrap_err()
        .to_string();
    assert!(err.contains("cancelada"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn orphan_holding_stdout_does_not_hang_a_finished_run() {
    let started = Instant::now();
    let result = pronto(run(&agent(), &session("orphan", "orphan"), AgentMode::Gerar).0);
    assert_eq!(result.url, "https://wiki.x/doc/a");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn success_text_mentioning_login_is_not_a_login_error() {
    let dir = session("okmentionslogin", "okmentionslogin");
    let result = pronto(run(&agent(), &dir, AgentMode::Gerar).0);
    assert_eq!(result.revision, 2);
}

#[test]
fn check_claude_reports_the_version_or_a_missing_install() {
    assert_eq!(
        check_claude(Path::new(FAKE)).unwrap(),
        "9.9.9 (Claude Code)"
    );
    let err = check_claude(Path::new(r"C:\nao-existe\claude.exe"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("não instalado"), "{err}");
}

fn args_vistos(dir: &Path) -> Vec<String> {
    let fake: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("fake-args.json")).unwrap())
            .unwrap();
    fake["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn perguntas_param_a_geracao_e_guardam_sessao_e_modo() {
    let dir = session("perg", "perguntas");
    let (r, _) = run(&agent(), &dir, AgentMode::Gerar);
    let AgentOutcome::Perguntas(p) = r.unwrap() else {
        panic!("esperava perguntas")
    };
    assert_eq!(p.perguntas[0].id, "q1");
    let salvo: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("perguntas.json")).unwrap())
            .unwrap();
    assert_eq!(
        (salvo["session_id"].as_str(), salvo["modo"].as_str()),
        (Some("sess-1"), Some("gerar"))
    );
    assert!(args_vistos(&dir).contains(&"Edit(./perguntas.json)".to_string()));
}

#[test]
fn continuar_usa_resume_e_arquiva_as_perguntas() {
    let dir = session("cont", "perguntas");
    run(&agent(), &dir, AgentMode::Gerar).0.unwrap();
    std::fs::write(
        dir.join("respostas.json"),
        r#"{"respostas":[{"id":"q1","escolhas":["ERP"]}]}"#,
    )
    .unwrap();
    let r = agent().continuar(&dir, &mut |_| {}).unwrap();
    assert!(matches!(r, AgentOutcome::Pronto(_)));
    let args = args_vistos(&dir);
    let i = args.iter().position(|a| a == "--resume").unwrap();
    assert_eq!(args[i + 1], "sess-1");
    assert!(
        args[1].contains("respostas.json") && args[1].contains("gerar"),
        "{}",
        args[1]
    );
    assert!(!dir.join("perguntas.json").exists() && !dir.join("respostas.json").exists());
    let logs: Vec<String> = std::fs::read_dir(dir.join("logs"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        logs.iter().any(|f| f.starts_with("perguntas-"))
            && logs.iter().any(|f| f.starts_with("respostas-")),
        "{logs:?}"
    );
}

#[test]
fn resume_que_falha_roda_do_zero() {
    let dir = session("resume-falha", "resume_falha");
    std::fs::write(dir.join("perguntas.json"), r#"{"perguntas":[{"id":"q1","pergunta":"?","opcoes":["a","b"]}],"session_id":"sess-1","modo":"melhoria"}"#).unwrap();
    std::fs::write(dir.join("respostas.json"), r#"{"pular":true}"#).unwrap();
    let r = agent().continuar(&dir, &mut |_| {}).unwrap();
    assert!(matches!(r, AgentOutcome::Pronto(_)));
    let args = args_vistos(&dir);
    assert_eq!(
        &args[..2],
        ["-p", "/gerar-manual melhoria"],
        "a 2ª rodada é do zero, no mesmo modo"
    );
}

#[test]
fn perguntar_de_novo_depois_das_respostas_e_erro() {
    let dir = session("de-novo", "pergunta_de_novo");
    std::fs::write(dir.join("perguntas.json"), r#"{"perguntas":[{"id":"q1","pergunta":"?","opcoes":["a","b"]}],"session_id":"sess-1","modo":"gerar"}"#).unwrap();
    std::fs::write(dir.join("respostas.json"), r#"{"pular":true}"#).unwrap();
    let e = agent().continuar(&dir, &mut |_| {}).unwrap_err();
    assert!(
        e.to_string()
            .contains("novas perguntas depois das respostas"),
        "{e}"
    );
    assert!(
        !dir.join("perguntas.json").exists(),
        "não fica em aguardando para sempre"
    );
}

#[test]
fn gerar_de_novo_descarta_perguntas_velhas() {
    let dir = session("velhas", "ok");
    std::fs::write(dir.join("perguntas.json"), "{}").unwrap();
    std::fs::write(dir.join("respostas.json"), "{}").unwrap();
    assert!(matches!(
        run(&agent(), &dir, AgentMode::Gerar).0.unwrap(),
        AgentOutcome::Pronto(_)
    ));
    assert!(!dir.join("perguntas.json").exists() && !dir.join("respostas.json").exists());
}
