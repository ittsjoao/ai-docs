mod common;

use common::*;
use screenmanual_core::domain::*;
use screenmanual_core::queries::*;

#[test]
fn lists_newest_first_with_derived_status() {
    let store = FakeStore::default();
    store.s.lock().unwrap().insert(
        "a".into(),
        Sess { meta: Some(meta("a", "Antiga", "2026-09-01T10:00:00-03:00")), candidates: Some(vec![]), ..Default::default() },
    );
    let mut p = PublishState::new("col");
    p.outline_id = Some("doc-1".into());
    p.url = Some("https://wiki/doc/doc-1".into());
    store.s.lock().unwrap().insert(
        "b".into(),
        Sess { meta: Some(meta("b", "Nova", "2026-09-29T10:00:00-03:00")), publish: Some(p), ..Default::default() },
    );
    let list = list_sessions(&store, &|id: &str| (id == "a").then_some(Activity::Generating)).unwrap();
    let got: Vec<(&str, SessionStatus)> = list.iter().map(|s| (s.id.as_str(), s.status)).collect();
    assert_eq!(got, vec![("b", SessionStatus::Draft), ("a", SessionStatus::Generating)]);
    assert_eq!(list[0].url.as_deref(), Some("https://wiki/doc/doc-1"));
}

#[test]
fn get_session_reports_error_message() {
    let store = FakeStore::with("a", Sess { meta: Some(meta("a", "X", "2026-09-01T10:00:00-03:00")), error: Some("falhou".into()), ..Default::default() });
    let s = get_session(&store, "a", None).unwrap();
    assert_eq!((s.status, s.error.as_deref()), (SessionStatus::Error, Some("falhou")));
}

#[test]
fn broken_session_is_listed_as_error() {
    let store = FakeStore::default();
    store.s.lock().unwrap().insert(
        "a".into(),
        Sess { meta: Some(meta("a", "Healthy", "2026-09-29T10:00:00-03:00")), candidates: Some(vec![]), ..Default::default() },
    );
    store.s.lock().unwrap().insert(
        "b".into(),
        Sess::default(),
    );
    let list = list_sessions(&store, &|_| None).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, "a");
    assert_eq!(list[0].status, SessionStatus::Ready);
    assert_eq!(list[1].id, "b");
    assert_eq!(list[1].title, "b");
    assert_eq!(list[1].status, SessionStatus::Error);
    assert!(list[1].error.is_some());
}
