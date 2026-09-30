use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;

use image::RgbaImage;
use screenmanual_core::domain::Event;

pub(crate) type PngJob = (PathBuf, RgbaImage);

/// Grava events.jsonl (uma linha por evento, com flush) e codifica PNGs numa thread própria,
/// para o worker voltar logo a capturar o próximo clique.
pub(crate) struct Sink {
    events: BufWriter<File>,
    png_tx: Option<Sender<PngJob>>,
    png_thread: Option<JoinHandle<()>>,
}

impl Sink {
    pub(crate) fn create(dir: &Path) -> io::Result<Sink> {
        std::fs::create_dir_all(dir.join("shots"))?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("events.jsonl"))?;
        let (tx, rx) = channel::<PngJob>();
        let png_thread = std::thread::spawn(move || {
            for (path, img) in rx {
                if let Err(e) = img.save(&path) {
                    eprintln!("falha ao salvar {}: {e}", path.display());
                }
            }
        });
        Ok(Sink {
            events: BufWriter::new(file),
            png_tx: Some(tx),
            png_thread: Some(png_thread),
        })
    }

    pub(crate) fn png_sender(&self) -> Sender<PngJob> {
        self.png_tx.clone().expect("sink já fechado")
    }

    pub(crate) fn write(&mut self, ev: &Event) -> io::Result<()> {
        serde_json::to_writer(&mut self.events, ev)?;
        self.events.write_all(b"\n")?;
        self.events.flush()
    }

    /// Espera os PNGs pendentes. Quem recebeu `png_sender` precisa ter descartado o seu antes.
    pub(crate) fn close(mut self) -> io::Result<()> {
        self.events.flush()?;
        drop(self.png_tx.take());
        if let Some(handle) = self.png_thread.take() {
            let _ = handle.join();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenmanual_core::domain::Event;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("smcap-{name}-{nanos}"))
    }

    #[test]
    fn writes_jsonl_lines_and_pngs() {
        let dir = temp_dir("sink");
        let mut sink = Sink::create(&dir).unwrap();
        sink.write(&Event::SessionStart { t: 0 }).unwrap();
        sink.write(&Event::Key {
            t: 10,
            combo: "Enter".into(),
        })
        .unwrap();
        let tx = sink.png_sender();
        tx.send((dir.join("shots/00000010.png"), RgbaImage::new(4, 4)))
            .unwrap();
        drop(tx);
        sink.close().unwrap();

        let text = std::fs::read_to_string(dir.join("events.jsonl")).unwrap();
        let events: Vec<Event> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(
            events,
            vec![
                Event::SessionStart { t: 0 },
                Event::Key {
                    t: 10,
                    combo: "Enter".into()
                }
            ]
        );
        assert!(dir.join("shots/00000010.png").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
