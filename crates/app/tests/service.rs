mod fakes;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fakes::*;
use screenmanual_app::*;
use screenmanual_core::commands::CommandError;
use screenmanual_core::domain::{Respostas, SessionStatus, TranscriptionModel};
use screenmanual_core::ports::{AgentOutcome, SessionStore};
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

#[test]
fn gera_e_mostra_os_avisos() {
    let (app, rx) = app("gerar");
    let id = pronta(&app, "Emitir NFS-e");
    let r = app.gerar(&id, "col", None, false).unwrap();
    assert_eq!(r.url().unwrap(), "http://wiki/doc/manual");
    assert!(
        !app.deps().store.dir(&id).join("instrucoes.txt").exists(),
        "a instrução extra não existe mais"
    );
    let d = app.detalhe(&id).unwrap();
    assert_eq!(d.resumo.status, SessionStatus::Draft);
    assert_eq!(d.colecao.as_deref(), Some("col"));
    assert_eq!(d.validacao[0].tipo, "aviso");
    let ev = eventos(&rx);
    assert!(ev
        .iter()
        .any(|e| matches!(e, Evento::Progresso { texto, .. } if texto == "lendo")));
    assert!(ev.iter().any(|e| matches!(
        e,
        Evento::Fim {
            acao: Acao::Gerar,
            url: Some(_),
            erro: None,
            ..
        }
    )));
}

#[test]
fn uma_geracao_por_vez_e_cancelar() {
    let (app, rx) = app("cancelar");
    let a = pronta(&app, "Primeira");
    let b = pronta(&app, "Segunda");
    eventos(&rx);
    *app.deps().plano.lock().unwrap() = Plano::EsperaCancelar;
    let (app2, a2) = (app.clone(), a.clone());
    let t = std::thread::spawn(move || app2.gerar(&a2, "col", None, false));
    loop {
        if let Evento::Progresso { texto, .. } = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("o agente não começou")
        {
            if texto == "lendo" {
                break;
            }
        }
    }
    assert_eq!(status(&app, &a), SessionStatus::Generating);
    assert_eq!(app.inicio().unwrap().gerando.as_deref(), Some(a.as_str()));
    assert!(app.ocupado());
    let e = app.gerar(&b, "col", None, false).unwrap_err();
    assert_eq!(e.kind, "ocupado");
    assert!(e.mensagem.contains("Primeira"), "{}", e.mensagem);
    assert_eq!(app.melhorar(&b, "x", false).unwrap_err().kind, "ocupado");
    assert_eq!(app.processar(&a, false).unwrap_err().kind, "ocupado");

    app.cancelar();
    let e = t.join().unwrap().unwrap_err();
    assert!(e.mensagem.contains("cancelada"), "{}", e.mensagem);
    assert_eq!(status(&app, &a), SessionStatus::Error);
    assert_eq!(app.inicio().unwrap().gerando, None);
    assert!(!app.ocupado());
}

#[test]
fn melhoria_sobre_edicao_manual_pede_confirmacao() {
    let (app, _rx) = app("d9");
    let id = pronta(&app, "Manual");
    app.gerar(&id, "col", None, false).unwrap();
    app.deps().remota.store(2, Ordering::SeqCst);
    let e = app.melhorar(&id, "troque o título", false).unwrap_err();
    assert_eq!(
        (e.kind, e.local, e.remote),
        ("editado_manualmente", Some(1), Some(2))
    );
    let feedback = app.deps().store.dir(&id).join("feedback.jsonl");
    assert!(!feedback.exists());
    app.melhorar(&id, "troque o título", true).unwrap();
    assert_eq!(
        std::fs::read_to_string(&feedback).unwrap().lines().count(),
        1
    );
}

#[test]
fn gerar_sobrescrevendo_aceita_a_revisao_remota() {
    let (app, _rx) = app("sobrescrever");
    let id = pronta(&app, "Manual");
    app.gerar(&id, "col", None, false).unwrap();
    app.deps().remota.store(3, Ordering::SeqCst);
    *app.deps().plano.lock().unwrap() = Plano::Editado;
    let r = app.gerar(&id, "col", None, false).unwrap();
    let AgentOutcome::Pronto(r) = r else {
        panic!("esperava Pronto")
    };
    assert_eq!(r.validacao[0].tipo, "editado_manualmente");
    assert_eq!(*app.deps().revisao_vista.lock().unwrap(), Some(1));
    *app.deps().plano.lock().unwrap() = Plano::Ok;
    app.gerar(&id, "col", None, true).unwrap();
    assert_eq!(*app.deps().revisao_vista.lock().unwrap(), Some(3));
}

#[test]
fn login_colecao_e_credenciais() {
    let (app, _rx) = app("login");
    let id = pronta(&app, "Manual");
    *app.deps().plano.lock().unwrap() = Plano::Login;
    assert_eq!(
        app.gerar(&id, "col", None, false).unwrap_err().kind,
        "login"
    );
    assert_eq!(
        app.gerar(&id, " ", None, false).unwrap_err().kind,
        "estado_invalido"
    );
    *app.deps().token.lock().unwrap() = None;
    assert_eq!(
        app.gerar(&id, "col", None, false).unwrap_err().kind,
        "estado_invalido"
    );
}

#[test]
fn aprovar_publica_o_rascunho() {
    let (app, _rx) = app("aprovar");
    let id = pronta(&app, "Manual");
    assert_eq!(app.aprovar(&id).unwrap_err().kind, "estado_invalido");
    app.gerar(&id, "col", None, false).unwrap();
    app.aprovar(&id).unwrap();
    assert_eq!(status(&app, &id), SessionStatus::Published);
}

#[test]
fn encerrar_para_a_gravacao_sem_processar() {
    let (app, _rx) = app("encerrar");
    let id = app.gravar("Manual", QUANDO).unwrap();
    app.encerrar().unwrap();
    assert!(!app.ocupado());
    assert_eq!(status(&app, &id), SessionStatus::Stopped);
    app.encerrar().unwrap(); // sem gravação: nada a fazer
}

#[test]
fn aprovar_reserva_a_sessao_e_emite_fim() {
    let (app, rx) = app("aprovar-reserva");
    let id = pronta(&app, "Manual");
    app.gerar(&id, "col", None, false).unwrap();
    eventos(&rx);
    app.aprovar(&id).unwrap();
    let ev = eventos(&rx);
    let sessoes = ev
        .iter()
        .filter(|e| matches!(e, Evento::Sessao { .. }))
        .count();
    assert_eq!(sessoes, 2, "reserva e liberação: {ev:?}");
    assert!(
        ev.iter().any(|e| matches!(
            e,
            Evento::Fim {
                acao: Acao::Aprovar,
                url: Some(_),
                erro: None,
                ..
            }
        )),
        "{ev:?}"
    );
    assert_eq!(app.aprovar(&id).unwrap_err().kind, "estado_invalido");
    assert!(
        eventos(&rx).iter().any(|e| matches!(
            e,
            Evento::Fim {
                acao: Acao::Aprovar,
                erro: Some(_),
                ..
            }
        )),
        "a falha também notifica"
    );
}

#[test]
fn textos_da_notificacao() {
    assert_eq!(
        texto_notificacao(Acao::Aprovar, "NFS-e", Some("http://w/d"), None),
        ("Manual publicado: NFS-e".into(), "http://w/d".into())
    );
    assert_eq!(
        texto_notificacao(Acao::Gerar, "NFS-e", Some("http://w/d"), None),
        ("Manual pronto: NFS-e".into(), "http://w/d".into())
    );
    assert_eq!(
        texto_notificacao(Acao::Processar, "NFS-e", None, None),
        (
            "Sessão processada: NFS-e".into(),
            "pronta para gerar o manual".into()
        )
    );
    assert_eq!(
        texto_notificacao(Acao::Melhorar, "NFS-e", None, Some("401")),
        ("Falhou: NFS-e".into(), "401".into())
    );
}

#[test]
fn aprovar_sessao_ocupada_nao_toca_no_error_txt() {
    let (app, _rx) = app("aprovar-ocupada");
    let id = app.gravar("A", QUANDO).unwrap();
    assert_eq!(app.aprovar(&id).unwrap_err().kind, "ocupado");
    assert!(!app.deps().store.dir(&id).join("error.txt").exists());
    app.parar().unwrap();
    assert_eq!(status(&app, &id), SessionStatus::Ready);
}

#[test]
fn id_invalido_e_recusado() {
    let (app, _rx) = app("id-invalido");
    let fora = format!("..{}x", char::from(92u8));
    let e = app.gerar(&fora, "col", None, false).unwrap_err();
    assert_eq!(
        (e.kind, e.mensagem.as_str()),
        ("estado_invalido", "sessão inválida")
    );
    assert_eq!(app.detalhe("C:").unwrap_err().kind, "estado_invalido");
    assert_eq!(app.detalhe("a/b").unwrap_err().kind, "estado_invalido");
    assert_eq!(app.detalhe("").unwrap_err().kind, "estado_invalido");
    assert_eq!(
        app.processar("..", false).unwrap_err().kind,
        "estado_invalido"
    );
    assert_eq!(
        app.melhorar("..", "x", false).unwrap_err().kind,
        "estado_invalido"
    );
    assert_eq!(app.aprovar("..").unwrap_err().kind, "estado_invalido");
}

#[test]
fn ocupado_vale_durante_o_processamento() {
    let root = temp("ocupado-proc");
    let config_file = root.join("config.json");
    AppConfig {
        outline_url: "http://wiki".into(),
        ..AppConfig::default()
    }
    .save(&config_file)
    .unwrap();
    let (tx, rx) = channel();
    let (go_tx, go_rx) = channel::<()>();
    let (tx, go_rx) = (Mutex::new(tx), Mutex::new(go_rx));
    let app = Arc::new(App::new(
        FakeDeps::new(&root.join("sessions")),
        config_file,
        Box::new(move |e| {
            let bloquear =
                matches!(&e, Evento::Progresso { texto, .. } if texto.starts_with("baixando"));
            let _ = tx.lock().unwrap().send(e);
            if bloquear {
                let _ = go_rx.lock().unwrap().recv_timeout(Duration::from_secs(5));
            }
        }),
    ));
    let id = app.gravar("Com fala", QUANDO).unwrap();
    std::fs::write(app.deps().store.dir(&id).join("audio.wav"), [0u8; 100]).unwrap();
    let app2 = app.clone();
    let t = std::thread::spawn(move || app2.parar());
    loop {
        if let Evento::Progresso { .. } = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("o processamento não começou")
        {
            break;
        }
    }
    assert!(app.ocupado(), "processando conta como ocupado");
    go_tx.send(()).unwrap();
    t.join().unwrap().unwrap();
    assert!(!app.ocupado());
}

#[test]
fn parar_sem_gravacao_nao_emite_nada() {
    // Sem gravação não há id para o Fim; o erro só volta ao chamador.
    let (app, rx) = app("parar-sem-gravacao");
    assert_eq!(app.parar().unwrap_err().kind, "estado_invalido");
    assert!(eventos(&rx).is_empty());
}

#[test]
fn gerar_em_subpagina_grava_o_pai() {
    let (app, _rx) = app("pai");
    let id = pronta(&app, "Manual");
    let arvore = app.documentos("col").unwrap();
    assert_eq!(arvore[0].children[0].id, "filho");
    app.gerar(&id, "col", Some("filho"), false).unwrap();
    assert_eq!(app.detalhe(&id).unwrap().pai.as_deref(), Some("filho"));
}

#[test]
fn pai_que_sumiu_do_outline_e_recusado() {
    let (app, _rx) = app("pai-sumiu");
    let id = pronta(&app, "Manual");
    let e = app.gerar(&id, "col", Some("apagado"), false).unwrap_err();
    assert_eq!(
        (e.kind, e.mensagem.as_str()),
        (
            "estado_invalido",
            "o documento escolhido não existe mais no Outline"
        )
    );
    assert_eq!(
        status(&app, &id),
        SessionStatus::Ready,
        "não reservou a sessão"
    );
}

/// PNG 1×1 válido.
fn png() -> Vec<u8> {
    let mut b = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(1, 1)
        .write_to(&mut b, image::ImageFormat::Png)
        .unwrap();
    b.into_inner()
}

#[test]
fn imagem_colada_entra_no_passo_e_republica() {
    let (app, rx) = app("imagens");
    let id = pronta(&app, "Manual");
    app.gerar(&id, "col", None, false).unwrap();
    let img = app.adicionar_imagem(&id, &png()).unwrap();
    assert_eq!(img, "u001");
    app.definir_imagem(&id, 1, Some(&img)).unwrap();
    assert_eq!(app.passos(&id).unwrap()[0].imagem.as_deref(), Some("u001"));
    assert!(app
        .imagens(&id)
        .unwrap()
        .iter()
        .any(|i| i.id == "u001" && i.origem == "operador"));
    assert_eq!(
        app.miniatura(&id, "u001").unwrap(),
        std::fs::read(app.deps().store.dir(&id).join("crops/u001.png")).unwrap()
    );
    eventos(&rx);
    app.republicar(&id, false).unwrap();
    assert!(eventos(&rx).iter().any(|e| matches!(
        e,
        Evento::Fim {
            acao: Acao::Republicar,
            url: Some(_),
            erro: None,
            ..
        }
    )));
}

#[test]
fn imagem_invalida_e_miniatura_estranha_sao_recusadas() {
    let (app, _rx) = app("imagens-ruins");
    let id = pronta(&app, "Manual");
    app.gerar(&id, "col", None, false).unwrap();
    let e = app.adicionar_imagem(&id, b"texto copiado").unwrap_err();
    assert!(
        e.mensagem.contains("não é uma imagem PNG ou JPEG"),
        "{}",
        e.mensagem
    );
    assert!(app.passos(&id).unwrap()[0].imagem.is_none());
    assert_eq!(
        app.miniatura(&id, "../session").unwrap_err().kind,
        "estado_invalido"
    );
    assert_eq!(
        app.miniatura(&id, "u009").unwrap_err().kind,
        "estado_invalido"
    );
}

fn erp() -> Respostas {
    serde_json::from_str(r#"{"respostas":[{"id":"q1","escolhas":["ERP"]}]}"#).unwrap()
}

#[test]
fn ia_pergunta_solta_a_trava_e_continua_com_as_respostas() {
    let (app, rx) = app("perguntas");
    let id = pronta(&app, "Manual");
    *app.deps().plano.lock().unwrap() = Plano::Perguntas;
    eventos(&rx); // descarta o Fim do processar de `pronta`
    let r = app.gerar(&id, "col", None, false).unwrap();
    assert!(matches!(r, AgentOutcome::Perguntas(_)));
    assert_eq!(status(&app, &id), SessionStatus::Awaiting);
    assert_eq!(app.inicio().unwrap().gerando, None, "a trava de geração foi solta");
    let ev = eventos(&rx);
    assert!(ev.iter().any(|e| matches!(e, Evento::Perguntas { .. })), "{ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, Evento::Fim { .. })), "perguntas não são fim: {ev:?}");
    assert_eq!(app.perguntas(&id).unwrap().unwrap().perguntas[0].id, "q1");

    assert_eq!(app.responder(&id, &Respostas::default()).unwrap_err().kind, "outro");
    let r = app.responder(&id, &erp()).unwrap();
    assert!(matches!(r, AgentOutcome::Pronto(_)));
    assert_eq!(status(&app, &id), SessionStatus::Draft);
    assert!(eventos(&rx).iter().any(|e| matches!(e, Evento::Fim { acao: Acao::Responder, url: Some(_), .. })));
}

#[test]
fn perguntas_sobrevivem_a_reinicio_do_app() {
    let (app, _rx) = app("perguntas-reinicio");
    let id = pronta(&app, "Manual");
    *app.deps().plano.lock().unwrap() = Plano::Perguntas;
    app.gerar(&id, "col", None, false).unwrap();
    let root = app.deps().store.dir(&id).parent().unwrap().parent().unwrap().to_path_buf();
    drop(app);
    let novo = App::new(
        FakeDeps::new(&root.join("sessions")),
        root.join("config.json"),
        Box::new(|_| {}),
    );
    assert_eq!(novo.detalhe(&id).unwrap().resumo.status, SessionStatus::Awaiting);
    assert!(matches!(novo.pular(&id).unwrap(), AgentOutcome::Pronto(_)));
}

#[test]
fn responder_duas_vezes_ao_mesmo_tempo_da_ocupado() {
    let (app, _rx) = app("perguntas-duplo");
    let id = pronta(&app, "Manual");
    *app.deps().plano.lock().unwrap() = Plano::Perguntas;
    app.gerar(&id, "col", None, false).unwrap();
    *app.deps().plano.lock().unwrap() = Plano::EsperaCancelar;
    let (a, i) = (app.clone(), id.clone());
    let t = std::thread::spawn(move || a.responder(&i, &erp()));
    for _ in 0..200 {
        if app.inicio().unwrap().gerando.is_some() { break; }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(app.responder(&id, &erp()).unwrap_err().kind, "ocupado");
    app.cancelar();
    assert!(t.join().unwrap().is_err());
}

#[test]
fn cancelar_perguntas_volta_a_pronta() {
    let (app, _rx) = app("perguntas-cancelar");
    let id = pronta(&app, "Manual");
    *app.deps().plano.lock().unwrap() = Plano::Perguntas;
    app.gerar(&id, "col", None, false).unwrap();
    app.cancelar_perguntas(&id).unwrap();
    assert_eq!(status(&app, &id), SessionStatus::Ready);
}
