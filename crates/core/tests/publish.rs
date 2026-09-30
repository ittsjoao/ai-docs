mod common;

use common::*;
use screenmanual_core::commands::*;
use screenmanual_core::domain::*;
use screenmanual_core::pipeline::render::RenderError;
use screenmanual_core::ports::*;

fn session_with_manual(img: &str) -> FakeStore {
    let mut s = Sess {
        candidates: Some(vec![cand("c001", Some("crops/c001.png"))]),
        manual: Some(manual_with_image(img)),
        publish: Some(PublishState::new("col-1")),
        ..Default::default()
    };
    s.files.insert("crops/c001.png".into(), b"png-1".to_vec());
    FakeStore::with("s1", s)
}

#[test]
fn first_publish_creates_draft_and_uploads_image() {
    let store = session_with_manual("c001");
    let wiki = FakeWiki::default();
    let st = publish_draft(&store, &wiki, "s1").unwrap();
    assert_eq!(st.outline_id.as_deref(), Some("doc-1"));
    assert_eq!((st.revision, st.status), (Some(1), Some(PublishStatus::Draft)));
    assert_eq!(wiki.uploads.lock().unwrap().len(), 1);
    let text = wiki.info("doc-1").unwrap().text;
    assert!(text.contains("](/api/attachments.redirect?id=att-1)"));
    assert!(!text.contains("sessao:"), "frontmatter removido");
    assert!(!text.starts_with("# Emitir NFS-e"), "título fica só no campo title do Outline: {text}");
    assert_eq!(store.get("s1").publish, Some(st));
}

#[test]
fn republish_updates_same_doc_without_reuploading() {
    let store = session_with_manual("c001");
    let wiki = FakeWiki::default();
    publish_draft(&store, &wiki, "s1").unwrap();
    let st = publish_draft(&store, &wiki, "s1").unwrap();
    assert_eq!((st.outline_id.as_deref(), st.revision), (Some("doc-1"), Some(2)));
    assert_eq!(wiki.uploads.lock().unwrap().len(), 1);
}

#[test]
fn external_edit_blocks_publish() {
    let store = session_with_manual("c001");
    let wiki = FakeWiki::default();
    publish_draft(&store, &wiki, "s1").unwrap();
    wiki.external_edit("doc-1");
    let err = publish_draft(&store, &wiki, "s1").unwrap_err();
    assert!(matches!(err, CommandError::EditedManually { local: 1, remote: 2 }), "{err}");
}

#[test]
fn render_and_state_errors() {
    let wiki = FakeWiki::default();
    let err = publish_draft(&session_with_manual("c999"), &wiki, "s1").unwrap_err();
    assert!(matches!(err, CommandError::Render(RenderError::UnknownCandidate { passo: 1, .. })));
    let mut s = Sess { candidates: Some(vec![]), ..Default::default() };
    let err = publish_draft(&FakeStore::with("s1", s.clone()), &wiki, "s1").unwrap_err();
    assert!(matches!(err, CommandError::NoSteps));
    s.manual = Some(Manual { secoes: vec![], ..manual_with_image("c001") });
    let err = publish_draft(&FakeStore::with("s1", s), &wiki, "s1").unwrap_err();
    assert!(matches!(err, CommandError::NoCollection));
}

#[test]
fn fetch_saves_published_copy_and_reports_integrity() {
    let store = session_with_manual("c001");
    let wiki = FakeWiki::default();
    publish_draft(&store, &wiki, "s1").unwrap();
    let report = fetch_published(&store, &wiki, "s1").unwrap();
    assert_eq!(report, FetchReport::default());
    let (md, images) = store.get("s1").published.unwrap();
    assert!(md.contains("Nova nota"));
    assert_eq!(images, vec![("att-1".to_string(), b"png-1".to_vec())]);
    wiki.uploads.lock().unwrap()[0].1 = b"corrompido".to_vec();
    assert_eq!(fetch_published(&store, &wiki, "s1").unwrap().mismatched, vec!["att-1"]);
}

#[test]
fn approve_publishes_draft_once() {
    let store = session_with_manual("c001");
    let wiki = FakeWiki::default();
    publish_draft(&store, &wiki, "s1").unwrap();
    let st = approve(&store, &wiki, "s1").unwrap();
    assert_eq!((st.status, st.revision), (Some(PublishStatus::Published), Some(2)));
    assert!(wiki.is_published("doc-1"));
    assert!(matches!(approve(&store, &wiki, "s1").unwrap_err(), CommandError::InvalidState(_)));
}

#[test]
fn approve_keeps_evidence_of_manual_edit() {
    let store = session_with_manual("c001");
    let wiki = FakeWiki::default();
    publish_draft(&store, &wiki, "s1").unwrap();
    wiki.external_edit("doc-1");
    let st = approve(&store, &wiki, "s1").unwrap();
    assert_eq!(st.revision, Some(1), "revisão guardada não avança");
    let agent = FakeAgent::ok();
    let err = improve_manual(&store, &wiki, &agent, "s1", "x", false, &mut |_| {}).unwrap_err();
    assert!(matches!(err, CommandError::EditedManually { .. }), "{err}");
}
