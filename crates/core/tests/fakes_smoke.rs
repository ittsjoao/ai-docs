mod common;

use common::*;
use screenmanual_core::ports::*;

#[test]
fn fake_wiki_tracks_revisions_and_attachments() {
    let wiki = FakeWiki::default();
    let doc = wiki
        .create_draft("col-1", None, "T", "📘", "texto")
        .unwrap();
    assert_eq!((doc.id.as_str(), doc.revision), ("doc-1", 1));
    wiki.external_edit("doc-1");
    assert_eq!(wiki.info("doc-1").unwrap().revision, 2);
    let att = wiki.upload_image(None, "c001.png", b"png").unwrap();
    assert_eq!(wiki.download_attachment(&att).unwrap(), b"png");
}

#[test]
fn fake_store_copies_rendered_images() {
    let mut s = Sess::default();
    s.files.insert("crops/c001.png".into(), b"png".to_vec());
    let store = FakeStore::with("s1", s);
    let r = screenmanual_core::domain::Rendered {
        markdown: "# T\n".into(),
        images: vec![screenmanual_core::domain::ImageCopy {
            from: "crops/c001.png".into(),
            to: "img/c001.png".into(),
        }],
    };
    store.save_rendered("s1", &r).unwrap();
    assert_eq!(store.read_file("s1", "img/c001.png").unwrap(), b"png");
}
