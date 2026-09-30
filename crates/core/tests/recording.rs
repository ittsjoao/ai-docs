mod common;

use common::*;
use screenmanual_core::commands::*;
use screenmanual_core::domain::CaptureConfig;

#[test]
fn start_creates_session_and_starts_recorder_in_its_dir() {
    let store = FakeStore::default();
    let rec = FakeRecorder::default();
    let (id, _h) = start_recording(
        &store,
        &rec,
        " Emitir NFS-e ",
        "2026-09-29T14:30:02-03:00",
        &CaptureConfig::default(),
    )
    .unwrap();
    assert_eq!(id, "2026-09-29_1430_emitir-nfs-e");
    assert_eq!(store.get(&id).meta.unwrap().title, "Emitir NFS-e");
    assert_eq!(
        rec.started.lock().unwrap()[0],
        std::path::PathBuf::from(&id)
    );
}

#[test]
fn same_minute_and_title_gets_suffix() {
    let store = FakeStore::default();
    let rec = FakeRecorder::default();
    let cfg = CaptureConfig::default();
    start_recording(
        &store,
        &rec,
        "Emitir NFS-e",
        "2026-09-29T14:30:02-03:00",
        &cfg,
    )
    .unwrap();
    let (id, _h) = start_recording(
        &store,
        &rec,
        "Emitir NFS-e",
        "2026-09-29T14:30:40-03:00",
        &cfg,
    )
    .unwrap();
    assert_eq!(id, "2026-09-29_1430_emitir-nfs-e-2");
}

#[test]
fn stop_saves_duration_and_audio_offset() {
    let store = FakeStore::default();
    let rec = FakeRecorder::default();
    let (id, h) = start_recording(
        &store,
        &rec,
        "X",
        "2026-09-29T14:30:02-03:00",
        &CaptureConfig::default(),
    )
    .unwrap();
    let stopped = h.stopped.clone();
    stop_recording(&store, &id, h).unwrap();
    let meta = store.get(&id).meta.unwrap();
    assert_eq!(
        (meta.duration_ms, meta.audio_offset_ms),
        (Some(60_000), Some(120))
    );
    assert!(*stopped.lock().unwrap());
}
