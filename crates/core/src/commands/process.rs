use super::error::finish;
use super::CommandResult;
use crate::domain::BuildConfig;
use crate::pipeline::build::build;
use crate::pipeline::transcript::{align, filter_hallucinations, pause_ranges, vocabulary};
use crate::ports::{Imaging, SessionStore, Transcriber};

/// Transcreve (com cache), monta candidatos e gera crops. Idempotente.
pub fn process_session<S: SessionStore, T: Transcriber, I: Imaging>(
    store: &S,
    transcriber: &T,
    imaging: &I,
    id: &str,
    cfg: &BuildConfig,
    force_transcribe: bool,
) -> CommandResult<usize> {
    let result = (|| -> CommandResult<usize> {
        let meta = store.meta(id)?;
        let events = store.events(id)?;
        let segments = match store.transcript(id)? {
            Some(cached) if !force_transcribe => cached,
            _ => {
                let segments = match store.audio_path(id)? {
                    Some(audio) => {
                        let raw = transcriber.transcribe(&audio, &vocabulary(&meta, &events))?;
                        filter_hallucinations(align(raw, meta.audio_offset_ms.unwrap_or(0)), &pause_ranges(&events))
                    }
                    None => vec![],
                };
                store.save_transcript(id, &segments)?;
                segments
            }
        };
        let out = build(&events, &segments, cfg);
        imaging.render_crops(&store.dir(id), &out.crops)?;
        store.save_candidates(id, &out.candidates)?;
        Ok(out.candidates.len())
    })();
    finish(store, id, result)
}
