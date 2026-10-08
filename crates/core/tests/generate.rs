mod common;

use common::*;
use screenmanual_core::commands::*;
use screenmanual_core::domain::*;
use screenmanual_core::ports::*;

fn ready() -> FakeStore {
    FakeStore::with(
        "s1",
        Sess {
            candidates: Some(vec![cand("c001", Some("crops/c001.png"))]),
            ..Default::default()
        },
    )
}

fn drafted(wiki: &FakeWiki) -> FakeStore {
    let doc = wiki
        .create_draft("col-1", None, "T", "📘", "texto")
        .unwrap();
    let mut st = PublishState::new("col-1");
    st.outline_id = Some(doc.id);
    st.revision = Some(doc.revision);
    st.status = Some(PublishStatus::Draft);
    FakeStore::with(
        "s1",
        Sess {
            candidates: Some(vec![]),
            publish: Some(st),
            ..Default::default()
        },
    )
}

#[test]
fn generate_requires_candidates() {
    let store = FakeStore::with("s1", Sess::default());
    let err =
        generate_manual(&store, &FakeAgent::ok(), "s1", "col-1", None, &mut |_| {}).unwrap_err();
    assert!(matches!(err, CommandError::InvalidState(_)));
}

#[test]
fn generate_saves_collection_and_runs_agent() {
    let store = ready();
    let agent = FakeAgent::ok();
    let mut seen = vec![];
    let res = generate_manual(&store, &agent, "s1", "col-9", None, &mut |m: &str| {
        seen.push(m.to_string())
    })
    .unwrap();
    assert!(matches!(res, AgentOutcome::Pronto(r) if r.revision == 2));
    assert_eq!(*agent.modes.lock().unwrap(), vec![AgentMode::Gerar]);
    assert_eq!(store.get("s1").publish.unwrap().collection_id, "col-9");
    assert_eq!(seen, vec!["trabalhando"]);
}

#[test]
fn agent_failure_is_recorded() {
    let store = ready();
    assert!(generate_manual(
        &store,
        &FakeAgent::failing("timeout"),
        "s1",
        "col-1",
        None,
        &mut |_| {}
    )
    .is_err());
    assert_eq!(store.get("s1").error.as_deref(), Some("timeout"));
}

#[test]
fn improve_appends_feedback_and_runs_melhoria() {
    let wiki = FakeWiki::default();
    let store = drafted(&wiki);
    let agent = FakeAgent::ok();
    improve_manual(
        &store,
        &wiki,
        &agent,
        "s1",
        "  junte os passos 3 e 4 ",
        false,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(store.get("s1").feedback, vec!["junte os passos 3 e 4"]);
    assert_eq!(*agent.modes.lock().unwrap(), vec![AgentMode::Melhoria]);
}

#[test]
fn improve_detects_manual_edit_and_honours_overwrite() {
    let wiki = FakeWiki::default();
    let store = drafted(&wiki);
    wiki.external_edit("doc-1");
    let agent = FakeAgent::ok();
    let err = improve_manual(&store, &wiki, &agent, "s1", "x", false, &mut |_| {}).unwrap_err();
    assert!(matches!(
        err,
        CommandError::EditedManually {
            local: 1,
            remote: 2
        }
    ));
    assert!(store.get("s1").feedback.is_empty());
    improve_manual(&store, &wiki, &agent, "s1", "x", true, &mut |_| {}).unwrap();
    assert_eq!(
        store.get("s1").publish.unwrap().revision,
        Some(2),
        "aceita a revisão remota"
    );
}

#[test]
fn improve_requires_text_and_draft() {
    let wiki = FakeWiki::default();
    let agent = FakeAgent::ok();
    assert!(matches!(
        improve_manual(
            &drafted(&wiki),
            &wiki,
            &agent,
            "s1",
            "  ",
            false,
            &mut |_| {}
        )
        .unwrap_err(),
        CommandError::InvalidState(_)
    ));
    assert!(matches!(
        improve_manual(&ready(), &wiki, &agent, "s1", "x", false, &mut |_| {}).unwrap_err(),
        CommandError::InvalidState(_)
    ));
}

#[test]
fn gerar_grava_o_pai_no_publish_json() {
    let store = ready();
    let agent = FakeAgent::ok();
    generate_manual(&store, &agent, "s1", "col-1", Some("pai-1"), &mut |_| {}).unwrap();
    assert_eq!(
        store
            .get("s1")
            .publish
            .unwrap()
            .parent_document_id
            .as_deref(),
        Some("pai-1")
    );
}

#[test]
fn responder_grava_as_respostas_e_continua() {
    let store = ready();
    store.edit_sess("s1", |s| s.perguntas = Some(perguntas()));
    let agent = FakeAgent::asking();
    let r: Respostas =
        serde_json::from_str(r#"{"respostas":[{"id":"q1","escolhas":["ERP"]}]}"#).unwrap();
    let out = answer_questions(&store, &agent, "s1", &r, &mut |_| {}).unwrap();
    assert!(matches!(out, AgentOutcome::Pronto(_)));
    assert_eq!(store.get("s1").respostas, Some(r));
}

#[test]
fn responder_incompleto_ou_sem_perguntas_e_recusado() {
    let store = ready();
    let agent = FakeAgent::asking();
    let vazia = Respostas::default();
    assert!(matches!(
        answer_questions(&store, &agent, "s1", &vazia, &mut |_| {}),
        Err(CommandError::InvalidState(_))
    ));
    store.edit_sess("s1", |s| s.perguntas = Some(perguntas()));
    let err = answer_questions(&store, &agent, "s1", &vazia, &mut |_| {}).unwrap_err();
    assert!(err.to_string().contains("Qual sistema?"), "{err}");
    assert_eq!(store.get("s1").respostas, None);
}

#[test]
fn cancelar_perguntas_volta_a_pronta() {
    let store = ready();
    store.edit_sess("s1", |s| s.perguntas = Some(perguntas()));
    assert!(store.facts("s1").unwrap().perguntas_pendentes);
    cancel_questions(&store, "s1").unwrap();
    assert!(!store.facts("s1").unwrap().perguntas_pendentes);
}
