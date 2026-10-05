mod fakes;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use fakes::*;
use screenmanual_app::*;
use screenmanual_core::commands::CommandError;
use screenmanual_core::domain::{SessionStatus, TranscriptionModel};
use screenmanual_core::ports::SessionStore;
use screenmanual_settings::AppConfig;

const QUANDO: &str = "2026-10-05T14:30:00-03:00";

fn temp(nome: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("smapp-{nome}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// App sobre fakes, com Outline configurado e token "tok"; os eventos caem no `Receiver`.
fn app(nome: &str) -> (Arc<App<FakeDeps>>, Receiver<Evento>) {
    let root = temp(nome);
    let config_file = root.join("config.json");
    AppConfig {
        outline_url: "http://wiki".into(),
        ..AppConfig::default()
    }
    .save(&config_file)
    .unwrap();
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let app = App::new(
        FakeDeps::new(&root.join("sessions")),
        config_file,
        Box::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
    );
    (Arc::new(app), rx)
}

fn eventos(rx: &Receiver<Evento>) -> Vec<Evento> {
    rx.try_iter().collect()
}

fn status(app: &App<FakeDeps>, id: &str) -> SessionStatus {
    app.detalhe(id).unwrap().resumo.status
}

/// Grava e para: a sessão sai processada (`pronta`).
#[allow(dead_code)]
fn pronta(app: &App<FakeDeps>, titulo: &str) -> String {
    app.gravar(titulo, QUANDO).unwrap();
    app.parar().unwrap()
}

#[test]
fn grava_pausa_marca_e_para_ate_pronta() {
    let (app, rx) = app("grava");
    let id = app.gravar("Emitir NFS-e", QUANDO).unwrap();
    assert_eq!(id, "2026-10-05_1430_emitir-nfs-e");
    assert_eq!(status(&app, &id), SessionStatus::Recording);
    assert_eq!(app.pausar().unwrap(), EstadoGravacao::Pausado);
    assert_eq!(app.marcar().unwrap_err().kind, "estado_invalido");
    assert_eq!(app.pausar().unwrap(), EstadoGravacao::Gravando);
    app.marcar().unwrap();
    assert_eq!(app.parar().unwrap(), id);
    assert_eq!(status(&app, &id), SessionStatus::Ready);
    assert_eq!(app.detalhe(&id).unwrap().candidatos, Some(0));
    assert_eq!(
        *app.deps().recorder.chamadas.lock().unwrap(),
        ["pause", "resume", "marker"]
    );
    assert!(
        app.deps().baixados.lock().unwrap().is_empty(),
        "sem áudio não baixa modelo"
    );
    let ev = eventos(&rx);
    let pos = |f: &dyn Fn(&Evento) -> bool| ev.iter().position(f).unwrap_or(usize::MAX);
    let parado = pos(&|e| {
        matches!(
            e,
            Evento::Gravacao {
                estado: EstadoGravacao::Parado,
                ..
            }
        )
    });
    let montando =
        pos(&|e| matches!(e, Evento::Progresso { texto, .. } if texto == "montando os passos"));
    let fim = pos(&|e| matches!(e, Evento::Fim { erro: None, .. }));
    assert!(
        parado < montando && montando < fim && fim < usize::MAX,
        "{ev:?}"
    );
}

#[test]
fn uma_gravacao_por_vez() {
    let (app, _rx) = app("ocupado");
    app.gravar("A", QUANDO).unwrap();
    assert!(app.ocupado());
    assert_eq!(app.gravar("B", QUANDO).unwrap_err().kind, "ocupado");
    app.parar().unwrap();
    assert!(!app.ocupado());
    assert_eq!(app.parar().unwrap_err().kind, "estado_invalido");
    assert_eq!(
        app.gravar("  ", QUANDO).unwrap_err().kind,
        "estado_invalido"
    );
}

#[test]
fn com_audio_baixa_o_modelo_antes_de_processar() {
    let (app, rx) = app("audio");
    let id = app.gravar("Com fala", QUANDO).unwrap();
    std::fs::write(app.deps().store.dir(&id).join("audio.wav"), [0u8; 100]).unwrap();
    app.parar().unwrap();
    assert_eq!(
        *app.deps().baixados.lock().unwrap(),
        [TranscriptionModel::Preciso]
    );
    assert!(eventos(&rx).iter().any(|e| matches!(e,
        Evento::Progresso { texto, .. } if texto == "baixando o modelo de transcrição: 100%")));
    assert_eq!(status(&app, &id), SessionStatus::Ready);
}

#[test]
fn conectar_outline_grava_token_e_url_so_se_der_certo() {
    let (app, _rx) = app("conectar");
    *app.deps().token.lock().unwrap() = None;
    assert!(!app.inicio().unwrap().configurado);
    assert!(app.conectar_outline("http://outro/", "ruim").is_err());
    assert_eq!(*app.deps().token.lock().unwrap(), None);
    assert_eq!(
        app.conectar_outline("http://outro", "").unwrap_err().kind,
        "estado_invalido"
    );
    let c = app.conectar_outline(" http://outro/ ", " novo ").unwrap();
    assert_eq!(c[0].id, "col");
    assert_eq!(app.deps().token.lock().unwrap().as_deref(), Some("novo"));
    assert_eq!(app.config().unwrap().outline_url, "http://outro");
    assert!(app.inicio().unwrap().configurado);
    // token vazio = mantém o guardado
    app.conectar_outline("http://outro", "").unwrap();
}

#[test]
fn inicio_mostra_claude_ausente_e_modelos() {
    let (app, rx) = app("inicio");
    *app.deps().claude.lock().unwrap() = Err("Claude Code não encontrado".into());
    let i = app.inicio().unwrap();
    assert_eq!(i.claude_versao, None);
    assert_eq!(i.claude_erro.as_deref(), Some("Claude Code não encontrado"));
    assert_eq!(i.gravacao, EstadoGravacao::Parado);
    assert_eq!(i.modelo_atual, TranscriptionModel::Preciso);
    assert!(i.modelos.iter().all(|m| !m.baixado));

    app.baixar_modelo(TranscriptionModel::Rapido).unwrap();
    assert!(app
        .inicio()
        .unwrap()
        .modelos
        .iter()
        .any(|m| m.modelo == TranscriptionModel::Rapido && m.baixado));
    let ev = eventos(&rx);
    assert!(ev.contains(&Evento::Modelo {
        modelo: TranscriptionModel::Rapido,
        baixado: 100,
        total: 100
    }));
    assert_eq!(
        ev.last(),
        Some(&Evento::ModeloFim {
            modelo: TranscriptionModel::Rapido,
            erro: None
        })
    );
}

#[test]
fn erros_viram_kinds_que_a_ui_entende() {
    let e = ApiError::from(CommandError::EditedManually {
        local: 1,
        remote: 2,
    });
    assert_eq!(
        (e.kind, e.local, e.remote),
        ("editado_manualmente", Some(1), Some(2))
    );
    let login = || anyhow::Error::from(screenmanual_agent::LoginRequired);
    assert_eq!(ApiError::from(CommandError::Unknown(login())).kind, "login");
    assert_eq!(ApiError::from(login()).kind, "login");
    assert_eq!(
        ApiError::from(CommandError::NoSteps).kind,
        "estado_invalido"
    );
    let e = ApiError::from(anyhow::anyhow!("raiz").context("camada"));
    assert_eq!((e.kind, e.mensagem.as_str()), ("outro", "camada: raiz"));
    let json = serde_json::to_value(ApiError::new("ocupado", "x")).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"kind": "ocupado", "mensagem": "x"})
    );
}

#[test]
fn processar_sessao_gravando_e_ocupado() {
    let (app, _rx) = app("proc-gravando");
    let id = app.gravar("A", QUANDO).unwrap();
    assert_eq!(app.processar(&id, false).unwrap_err().kind, "ocupado");
    assert_eq!(app.parar().unwrap(), id);
    assert_eq!(status(&app, &id), SessionStatus::Ready);
}
