use super::CommandResult;
use crate::domain::{session_id, CaptureConfig, SessionMeta, SCHEMA_VERSION};
use crate::ports::{Recorder, RecordingHandle, SessionStore};

pub fn start_recording<S: SessionStore, R: Recorder>(
    store: &S,
    recorder: &R,
    title: &str,
    started_at: &str,
    cfg: &CaptureConfig,
) -> CommandResult<(String, R::Handle)> {
    let base = session_id(started_at, title);
    let existing = store.list_ids()?;
    let mut id = base.clone();
    let mut n = 2;
    while existing.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    store.create(&SessionMeta {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        title: title.trim().to_string(),
        started_at: started_at.to_string(),
        audio_offset_ms: None,
        duration_ms: None,
    })?;
    let handle = recorder.start(&store.dir(&id), cfg)?;
    Ok((id, handle))
}

pub fn stop_recording<S: SessionStore, H: RecordingHandle>(store: &S, id: &str, handle: H) -> CommandResult<()> {
    let info = handle.stop()?;
    let mut meta = store.meta(id)?;
    meta.duration_ms = Some(info.duration_ms);
    meta.audio_offset_ms = info.audio_offset_ms;
    store.save_meta(&meta)?;
    Ok(())
}
