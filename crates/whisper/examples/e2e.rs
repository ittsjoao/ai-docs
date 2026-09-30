//! Grava uma sessão narrada, para e processa sobre os adapters reais:
//! cargo run -p screenmanual-whisper --example e2e --release -- <pasta-raiz> <modelo.bin> [título]
use std::io::BufRead;
use std::path::PathBuf;
use std::time::Instant;

use screenmanual_capture::{set_dpi_awareness, WinRecorder};
use screenmanual_core::commands::{process_session, start_recording, stop_recording};
use screenmanual_core::domain::{BuildConfig, CaptureConfig};
use screenmanual_core::ports::SessionStore;
use screenmanual_core::queries::get_session;
use screenmanual_store::{utc_now_rfc3339, FsStore, ImageCrops};
use screenmanual_whisper::WhisperTranscriber;

fn main() -> anyhow::Result<()> {
    set_dpi_awareness();
    let mut args = std::env::args().skip(1);
    let usage = "uso: e2e <pasta-raiz> <modelo.bin> [título]";
    let root = args.next().ok_or_else(|| anyhow::anyhow!(usage))?;
    let model = args.next().ok_or_else(|| anyhow::anyhow!(usage))?;
    let title = args
        .next()
        .unwrap_or_else(|| "Teste de ponta a ponta".into());
    let store = FsStore::new(&root);

    // ponytail: hora UTC no id da sessão; o app (plano 05) passa a hora local com fuso
    let (id, handle) = start_recording(
        &store,
        &WinRecorder,
        &title,
        &utc_now_rfc3339(),
        &CaptureConfig::default(),
    )?;
    println!(
        "gravando {} — Enter vazio para parar",
        store.dir(&id).display()
    );
    for line in std::io::stdin().lock().lines() {
        if line?.trim().is_empty() {
            break;
        }
    }
    stop_recording(&store, &id, handle)?;
    println!("{:?}", store.meta(&id)?);

    println!("processando (transcrição + candidatos + recortes)…");
    let started = Instant::now();
    let transcriber = WhisperTranscriber::new(PathBuf::from(model));
    let n = process_session(
        &store,
        &transcriber,
        &ImageCrops,
        &id,
        &BuildConfig::default(),
        false,
    )?;
    println!(
        "{n} candidatos em {:.1}s — status {:?}",
        started.elapsed().as_secs_f32(),
        get_session(&store, &id, None)?.status
    );
    for s in store.transcript(&id)?.unwrap_or_default() {
        println!("[{:>7}–{:>7}] {}", s.start, s.end, s.text);
    }
    for c in store.candidates(&id)? {
        let el =
            c.el.as_ref()
                .map(|e| format!("{} [{:?}]", e.name, e.quality))
                .unwrap_or_default();
        println!(
            "{} {:?} | {} | {} | {} | fala: {}",
            c.id,
            c.kind,
            c.window,
            el,
            c.crop.unwrap_or_default(),
            c.speech.join(" / ")
        );
    }
    Ok(())
}
