mod common;

use common::*;
use screenmanual_core::commands::*;
use screenmanual_core::domain::*;
use screenmanual_core::ports::*;

fn publicada() -> (FakeStore, FakeWiki) {
    let mut s = Sess {
        candidates: Some(vec![cand("c001", Some("crops/c001.png"))]),
        manual: Some(manual_with_image("c001")),
        publish: Some(PublishState::new("col-1")),
        ..Default::default()
    };
    s.files.insert("crops/c001.png".into(), b"png-1".to_vec());
    let store = FakeStore::with("s1", s);
    let wiki = FakeWiki::default();
    publish_draft(&store, &wiki, "s1").unwrap();
    (store, wiki)
}

#[test]
fn so_local_apaga_a_sessao_e_mantem_o_documento() {
    let (store, wiki) = publicada();
    delete_session::<_, FakeWiki>(&store, None, "s1").unwrap();
    assert!(store.list_ids().unwrap().is_empty());
    assert!(wiki.info("doc-1").is_ok());
}

#[test]
fn com_outline_apaga_o_documento_e_a_sessao() {
    let (store, wiki) = publicada();
    delete_session(&store, Some(&wiki), "s1").unwrap();
    assert!(store.list_ids().unwrap().is_empty());
    assert_eq!(*wiki.deleted.lock().unwrap(), vec!["doc-1".to_string()]);
}

#[test]
fn outline_falhando_nao_apaga_nada() {
    let (store, wiki) = publicada();
    *wiki.fail_delete.lock().unwrap() = true;
    assert!(delete_session(&store, Some(&wiki), "s1").is_err());
    assert_eq!(store.list_ids().unwrap(), vec!["s1".to_string()]);
}

#[test]
fn sem_documento_com_outline_apaga_so_o_local() {
    let store = FakeStore::with("s1", Sess { candidates: Some(vec![]), ..Default::default() });
    let wiki = FakeWiki::default();
    delete_session(&store, Some(&wiki), "s1").unwrap();
    assert!(store.list_ids().unwrap().is_empty());
    assert!(wiki.deleted.lock().unwrap().is_empty());
}
