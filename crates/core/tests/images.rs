mod common;

use common::*;
use screenmanual_core::commands::*;
use screenmanual_core::domain::*;

fn sessao() -> FakeStore {
    let mut s = Sess {
        candidates: Some(vec![cand("c001", Some("crops/c001.png")), cand("c002", None)]),
        manual: Some(manual_with_image("c001")),
        publish: Some(PublishState::new("col-1")),
        ..Default::default()
    };
    s.files.insert("crops/c001.png".into(), b"png-1".to_vec());
    FakeStore::with("s1", s)
}

#[test]
fn troca_e_tira_a_imagem_de_um_passo() {
    let store = sessao();
    set_step_image(&store, "s1", 1, None).unwrap();
    assert_eq!(store.get("s1").manual.unwrap().secoes[0].passos[0].imagem, None);
    set_step_image(&store, "s1", 1, Some("c001")).unwrap();
    assert_eq!(
        store.get("s1").manual.unwrap().secoes[0].passos[0].imagem.as_deref(),
        Some("c001")
    );
}

#[test]
fn recusa_imagem_sem_crop_id_estranho_e_passo_fora() {
    let store = sessao();
    assert!(matches!(set_step_image(&store, "s1", 1, Some("c002")), Err(CommandError::InvalidState(_))));
    assert!(matches!(set_step_image(&store, "s1", 1, Some("../x")), Err(CommandError::InvalidState(_))));
    assert!(matches!(set_step_image(&store, "s1", 2, None), Err(CommandError::InvalidState(_))));
    assert!(matches!(set_step_image(&store, "s1", 0, None), Err(CommandError::InvalidState(_))));
}

#[test]
fn imagem_do_operador_entra_em_extras() {
    let store = sessao();
    let id = add_operator_image(&store, "s1", b"bytes").unwrap();
    assert_eq!(id, "u001");
    assert_eq!(store.get("s1").manual.unwrap().extras, vec!["u001"]);
    set_step_image(&store, "s1", 1, Some("u001")).unwrap();
    assert_eq!(image_path(&store, "s1", "u001").unwrap(), "crops/u001.png");
    assert_eq!(image_path(&store, "s1", "c001").unwrap(), "crops/c001.png");
    assert!(image_path(&store, "s1", "c002").is_err());
}

#[test]
fn imagem_grande_demais_e_recusada_sem_gravar() {
    let store = sessao();
    let grande = vec![0u8; MAX_IMAGEM + 1];
    assert!(matches!(add_operator_image(&store, "s1", &grande), Err(CommandError::InvalidState(_))));
    assert!(store.get("s1").manual.unwrap().extras.is_empty());
}

#[test]
fn republicar_sobrescrevendo_alinha_a_revisao() {
    let store = sessao();
    let wiki = FakeWiki::default();
    publish_draft(&store, &wiki, "s1").unwrap();
    wiki.external_edit("doc-1");
    assert!(matches!(republish(&store, &wiki, "s1", false), Err(CommandError::EditedManually { .. })));
    let st = republish(&store, &wiki, "s1", true).unwrap();
    assert_eq!(st.revision, Some(3));
}
