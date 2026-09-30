//! Gravação manual para validar a captura:
//! cargo run -p screenmanual-capture --example record --release -- <pasta>
use std::io::BufRead;
use std::path::PathBuf;

use screenmanual_capture::{set_dpi_awareness, WinRecorder};
use screenmanual_core::domain::{BuildConfig, CaptureConfig, Event};
use screenmanual_core::pipeline::build::build;
use screenmanual_core::ports::{Recorder, RecordingHandle};

fn main() -> anyhow::Result<()> {
    set_dpi_awareness();
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "rec_out".into()));
    let handle = WinRecorder.start(&dir, &CaptureConfig::default())?;
    println!(
        "gravando em {} — Enter vazio para parar, p = pausa/retoma, m = marca passo",
        dir.display()
    );
    let mut paused = false;
    for line in std::io::stdin().lock().lines() {
        match line?.trim() {
            "" => break,
            "p" => {
                if paused {
                    handle.resume()
                } else {
                    handle.pause()
                }
                paused = !paused;
                println!("{}", if paused { "pausado" } else { "retomado" });
            }
            "m" => {
                handle.marker();
                println!("passo marcado");
            }
            other => println!("comando desconhecido: {other}"),
        }
    }
    let info = handle.stop()?;
    println!("{info:?}");

    let text = std::fs::read_to_string(dir.join("events.jsonl"))?;
    let events: Vec<Event> = text
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let out = build(&events, &[], &BuildConfig::default());
    println!(
        "{} eventos, {} candidatos, {} crops",
        events.len(),
        out.candidates.len(),
        out.crops.len()
    );
    for c in &out.candidates {
        let el =
            c.el.as_ref()
                .map(|e| format!("{} [{:?}]", e.name, e.quality))
                .unwrap_or_default();
        println!(
            "{} {:?} | {} | {} | {}",
            c.id,
            c.kind,
            c.window,
            el,
            c.keys.join(",")
        );
    }
    Ok(())
}
