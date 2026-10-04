//! Reprocessa uma sessão já gravada com outro modelo (sobrescreve transcript, candidatos e recortes):
//! cargo run -p screenmanual-whisper --example reprocess --release -- <pasta-raiz> <id> <pasta-modelos> <rapido|equilibrado|preciso> [--vocabulario]
use std::path::Path;
use std::time::Instant;

use screenmanual_core::commands::process_session;
use screenmanual_core::domain::{BuildConfig, TranscribeConfig};
use screenmanual_core::ports::SessionStore;
use screenmanual_store::{FsStore, ImageCrops};
use screenmanual_whisper::WhisperTranscriber;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "uso: reprocess <pasta-raiz> <id> <pasta-modelos> <rapido|equilibrado|preciso> [--vocabulario]";
    let [root, id, models, modelo, rest @ ..] = args.as_slice() else {
        anyhow::bail!(usage)
    };
    let cfg: TranscribeConfig = serde_json::from_value(serde_json::json!({
        "modelo": modelo,
        "vocabulario": rest.iter().any(|a| a == "--vocabulario"),
    }))?;
    let store = FsStore::new(root);
    let transcriber = WhisperTranscriber::from_config(Path::new(models), &cfg);
    let duration = store.meta(id)?.duration_ms.unwrap_or(0) as f32 / 1000.0;
    let started = Instant::now();
    let n = process_session(
        &store,
        &transcriber,
        &ImageCrops,
        id,
        &BuildConfig::default(),
        true,
    )?;
    let secs = started.elapsed().as_secs_f32();
    println!(
        "{cfg:?}: {n} candidatos, {secs:.1}s para {duration:.1}s de sessão (RTF {:.2})",
        secs / duration.max(1.0)
    );
    for s in store.transcript(id)?.unwrap_or_default() {
        println!("[{:>7}–{:>7}] {}", s.start, s.end, s.text);
    }
    Ok(())
}
