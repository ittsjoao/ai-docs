//! process_session sobre os adapters reais de arquivo, com uma pasta como a que a captura grava.
use std::path::Path;

use screenmanual_core::commands::process_session;
use screenmanual_core::domain::*;
use screenmanual_core::ports::{PortResult, SessionStore, Transcriber};
use screenmanual_core::queries::get_session;
use screenmanual_store::{FsStore, ImageCrops};

struct NoAudio;

impl Transcriber for NoAudio {
    fn transcribe(&self, _audio: &Path, _prompt: &str) -> PortResult<Vec<Segment>> {
        anyhow::bail!("sessão sem áudio não deveria ser transcrita")
    }
}

#[test]
fn processes_a_recorded_session_folder_without_audio() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("smprocess-{nanos}"));
    let store = FsStore::new(&root);
    store
        .create(&SessionMeta {
            schema_version: SCHEMA_VERSION,
            id: "s1".into(),
            title: "Emitir NFS-e".into(),
            started_at: "2026-09-30T14:03:05-03:00".into(),
            audio_offset_ms: None,
            duration_ms: Some(9000),
        })
        .unwrap();
    let dir = store.dir("s1");
    std::fs::create_dir_all(dir.join("shots")).unwrap();
    image::RgbaImage::from_pixel(400, 300, image::Rgba([255, 255, 255, 255]))
        .save(dir.join("shots").join("00003000.png"))
        .unwrap();
    let monitor = Rect {
        left: 0,
        top: 0,
        right: 400,
        bottom: 300,
    };
    let events = [
        Event::SessionStart { t: 0 },
        Event::Window {
            t: 0,
            app: "erp.exe".into(),
            title: "ERP".into(),
            url: None,
        },
        Event::Click {
            t: 3000,
            button: MouseButton::Left,
            x: 200,
            y: 150,
            up_x: 200,
            up_y: 150,
            shot: Some("shots/00003000.png".into()),
            dhash: Some(1),
            el: None,
            monitor,
        },
        Event::SessionEnd { t: 9000 },
    ];
    let mut lines: String = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap() + "\n")
        .collect();
    lines.push_str("{\"type\":\"cli"); // última linha cortada por um crash
    std::fs::write(dir.join("events.jsonl"), lines).unwrap();

    let n = process_session(
        &store,
        &NoAudio,
        &ImageCrops,
        "s1",
        &BuildConfig::default(),
        false,
    )
    .unwrap();

    let candidates = store.candidates("s1").unwrap();
    assert_eq!(candidates.len(), n);
    let crops: Vec<&String> = candidates
        .iter()
        .flat_map(|c| c.crop.iter().chain(c.context_shot.iter()))
        .collect();
    assert!(!crops.is_empty(), "o clique gera recorte");
    for rel in crops {
        let img = image::open(dir.join(rel)).unwrap();
        assert!(img.width() <= 1280 && img.height() <= 1280, "{rel}");
    }
    assert_eq!(store.transcript("s1").unwrap(), Some(vec![]));
    assert_eq!(
        get_session(&store, "s1", None).unwrap().status,
        SessionStatus::Ready
    );
    std::fs::remove_dir_all(root).unwrap();
}
