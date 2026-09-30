mod common;

use common::*;
use screenmanual_core::commands::*;
use screenmanual_core::domain::*;

fn session(has_audio: bool) -> FakeStore {
    let mut m = meta("s1", "Emitir NFS-e", "2026-09-29T14:30:02-03:00");
    m.audio_offset_ms = Some(500);
    let s = Sess {
        meta: Some(m),
        events: vec![
            Event::Window {
                t: 0,
                app: "erp.exe".into(),
                title: "ERP".into(),
                url: None,
            },
            click_ev(3000),
            Event::SessionEnd { t: 9000 },
        ],
        has_audio,
        ..Default::default()
    };
    FakeStore::with("s1", s)
}

fn seg(start: u64, end: u64, text: &str) -> Segment {
    Segment {
        start,
        end,
        text: text.into(),
        words: vec![],
    }
}

#[test]
fn transcribes_aligns_filters_builds_and_crops() {
    let store = session(true);
    let tr = FakeTranscriber::ok(vec![
        seg(1000, 2000, "clico em nova nota"),
        seg(2000, 2500, "Legendas pela comunidade Amara.org"),
    ]);
    let img = FakeImaging::default();
    let n = process_session(&store, &tr, &img, "s1", &BuildConfig::default(), false).unwrap();
    assert_eq!(n, 2, "switch inicial + clique");
    let saved = store.get("s1");
    assert_eq!(
        saved.transcript.unwrap(),
        vec![seg(1500, 2500, "clico em nova nota")]
    );
    assert_eq!(
        saved.candidates.unwrap()[1].speech,
        vec!["clico em nova nota"]
    );
    assert_eq!(img.specs.lock().unwrap().len(), 2, "crop + contexto");
    assert!(tr.prompts.lock().unwrap()[0].starts_with("Emitir NFS-e, ERP"));
    assert_eq!(saved.error, None);
}

#[test]
fn cached_transcript_is_reused_unless_forced() {
    let store = session(true);
    let tr = FakeTranscriber::ok(vec![]);
    let img = FakeImaging::default();
    let cfg = BuildConfig::default();
    process_session(&store, &tr, &img, "s1", &cfg, false).unwrap();
    process_session(&store, &tr, &img, "s1", &cfg, false).unwrap();
    assert_eq!(tr.calls(), 1);
    process_session(&store, &tr, &img, "s1", &cfg, true).unwrap();
    assert_eq!(tr.calls(), 2);
}

#[test]
fn without_audio_saves_empty_transcript() {
    let store = session(false);
    let tr = FakeTranscriber::ok(vec![seg(0, 1, "não deveria")]);
    process_session(
        &store,
        &tr,
        &FakeImaging::default(),
        "s1",
        &BuildConfig::default(),
        false,
    )
    .unwrap();
    assert_eq!(tr.calls(), 0);
    assert_eq!(store.get("s1").transcript, Some(vec![]));
}

#[test]
fn failure_is_recorded_on_session() {
    let store = session(true);
    let err = process_session(
        &store,
        &FakeTranscriber::failing("modelo ausente"),
        &FakeImaging::default(),
        "s1",
        &BuildConfig::default(),
        false,
    );
    assert!(err.is_err());
    assert_eq!(store.get("s1").error.as_deref(), Some("modelo ausente"));
}
