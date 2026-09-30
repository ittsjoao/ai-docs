use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use screenmanual_core::domain::{CaptureConfig, Event};
use screenmanual_core::ports::{PortResult, Recorder, RecordingHandle, StopInfo};

use crate::aggregate::{Aggregator, Raw};
use crate::audio::{self, Audio};
use crate::hooks::{self, Hooks};
use crate::sink::Sink;
use crate::win::WinProbe;

/// Frequência de checagem da janela ativa (spec §5).
const TICK: Duration = Duration::from_millis(250);

fn now(t0: Instant) -> u64 {
    t0.elapsed().as_millis() as u64
}

pub struct WinRecorder;

pub struct WinHandle {
    tx: Sender<Raw>,
    t0: Instant,
    hooks: Option<Hooks>,
    worker: Option<JoinHandle<anyhow::Result<StopInfo>>>,
}

impl Recorder for WinRecorder {
    type Handle = WinHandle;

    fn start(&self, dir: &Path, cfg: &CaptureConfig) -> PortResult<WinHandle> {
        std::fs::create_dir_all(dir)?;
        let t0 = Instant::now();
        let (tx, rx) = channel::<Raw>();
        let (ready_tx, ready_rx) = channel::<anyhow::Result<()>>();
        let (dir, cfg) = (dir.to_path_buf(), cfg.clone());
        let worker = std::thread::spawn(move || run(rx, dir, cfg, t0, ready_tx));
        ready_rx
            .recv()
            .map_err(|_| anyhow!("a captura terminou antes de iniciar"))??;
        match hooks::start(tx.clone(), t0) {
            Ok(h) => Ok(WinHandle {
                tx,
                t0,
                hooks: Some(h),
                worker: Some(worker),
            }),
            Err(e) => {
                let _ = tx.send(Raw::Stop { t: now(t0) });
                let _ = worker.join();
                Err(e)
            }
        }
    }
}

impl RecordingHandle for WinHandle {
    fn pause(&self) {
        let _ = self.tx.send(Raw::Pause { t: now(self.t0) });
    }

    fn resume(&self) {
        let _ = self.tx.send(Raw::Resume { t: now(self.t0) });
    }

    fn marker(&self) {
        let _ = self.tx.send(Raw::Marker { t: now(self.t0) });
    }

    fn stop(mut self) -> PortResult<StopInfo> {
        self.finish()
    }
}

impl WinHandle {
    fn finish(&mut self) -> PortResult<StopInfo> {
        if let Some(h) = self.hooks.take() {
            h.stop();
        }
        let worker = self
            .worker
            .take()
            .ok_or_else(|| anyhow!("gravação já encerrada"))?;
        let _ = self.tx.send(Raw::Stop { t: now(self.t0) });
        worker
            .join()
            .map_err(|_| anyhow!("a thread de captura entrou em pânico"))?
    }
}

impl Drop for WinHandle {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn write_all(sink: &mut Sink, events: Vec<Event>, audio: Option<&Audio>) -> anyhow::Result<()> {
    for ev in &events {
        if let Some(a) = audio {
            match ev {
                Event::Pause { .. } => a.set_paused(true),
                Event::Resume { .. } => a.set_paused(false),
                _ => {}
            }
        }
        sink.write(ev)?;
    }
    Ok(())
}

fn init(dir: &Path) -> anyhow::Result<(Sink, WinProbe)> {
    let sink = Sink::create(dir)?;
    let probe = WinProbe::new(dir, sink.png_sender())?;
    Ok((sink, probe))
}

fn run(
    rx: Receiver<Raw>,
    dir: PathBuf,
    cfg: CaptureConfig,
    t0: Instant,
    ready: Sender<anyhow::Result<()>>,
) -> anyhow::Result<StopInfo> {
    let (mut sink, probe) = match init(&dir) {
        Ok(v) => {
            let _ = ready.send(Ok(()));
            v
        }
        Err(e) => {
            let message = e.to_string();
            let _ = ready.send(Err(e));
            return Err(anyhow!(message));
        }
    };
    let audio = match audio::start(dir.join("audio.wav"), t0) {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!("gravando sem áudio: {e}");
            None
        }
    };
    let mut agg = Aggregator::new(probe, cfg, std::process::id());
    let end = write_all(&mut sink, agg.start(), audio.as_ref())
        .and_then(|_| pump(&rx, &mut agg, &mut sink, audio.as_ref(), t0));
    drop(agg); // libera o sender de PNG do probe antes de fechar o sink
    let closed = sink.close();
    let stopped = match audio {
        Some(a) => a.stop(),
        None => Ok(None),
    };
    let duration_ms = end?;
    closed?;
    let audio_offset_ms = stopped?;
    Ok(StopInfo {
        duration_ms,
        audio_offset_ms,
    })
}

fn pump(
    rx: &Receiver<Raw>,
    agg: &mut Aggregator<WinProbe>,
    sink: &mut Sink,
    audio: Option<&Audio>,
    t0: Instant,
) -> anyhow::Result<u64> {
    let mut audio_lost_reported = false;
    let mut last_tick = Instant::now();
    loop {
        let raw = match rx.recv_timeout(TICK) {
            Ok(raw) => raw,
            Err(RecvTimeoutError::Timeout) => {
                last_tick = Instant::now();
                Raw::Tick { t: now(t0) }
            }
            Err(RecvTimeoutError::Disconnected) => Raw::Stop { t: now(t0) },
        };
        let stop_at = match raw {
            Raw::Stop { t } => Some(t),
            _ => None,
        };
        write_all(sink, agg.feed(raw), audio)?;
        if let Some(t) = stop_at {
            return Ok(t);
        }
        if !audio_lost_reported && audio.is_some_and(Audio::lost) {
            audio_lost_reported = true;
            write_all(sink, agg.feed(Raw::AudioLost { t: now(t0) }), audio)?;
        }
        if last_tick.elapsed() >= TICK {
            last_tick = Instant::now();
            write_all(sink, agg.feed(Raw::Tick { t: now(t0) }), audio)?;
        }
    }
}
