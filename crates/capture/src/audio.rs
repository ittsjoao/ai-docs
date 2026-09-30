use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use anyhow::{anyhow, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

type Wav = hound::WavWriter<BufWriter<File>>;

fn f32_sample(s: f32) -> f32 {
    s
}

fn i16_sample(s: i16) -> f32 {
    s as f32 / i16::MAX as f32
}

/// Mistura para mono em PCM 16-bit. Em pausa grava zeros, para o áudio continuar alinhado aos eventos.
pub(crate) fn mixdown<T: Copy>(
    data: &[T],
    channels: usize,
    paused: bool,
    to_f32: fn(T) -> f32,
) -> Vec<i16> {
    data.chunks(channels.max(1))
        .map(|frame| {
            if paused {
                return 0;
            }
            let avg = frame.iter().map(|s| to_f32(*s)).sum::<f32>() / frame.len() as f32;
            (avg.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
        })
        .collect()
}

/// Instante (ms desde t0) em que foi capturada a primeira amostra do primeiro bloco.
pub(crate) fn first_block_offset_ms(elapsed_ms: i64, frames: usize, rate: u32) -> i64 {
    elapsed_ms - (frames as i64 * 1000 / rate.max(1) as i64)
}

struct Shared {
    writer: Mutex<Option<Wav>>,
    offset: Mutex<Option<i64>>,
    paused: AtomicBool,
    lost: AtomicBool,
}

impl Shared {
    fn on_block<T: Copy>(
        &self,
        data: &[T],
        channels: usize,
        rate: u32,
        t0: Instant,
        to_f32: fn(T) -> f32,
    ) {
        {
            let mut offset = self.offset.lock().unwrap();
            if offset.is_none() {
                *offset = Some(first_block_offset_ms(
                    t0.elapsed().as_millis() as i64,
                    data.len() / channels.max(1),
                    rate,
                ));
            }
        }
        let samples = mixdown(data, channels, self.paused.load(Ordering::Relaxed), to_f32);
        if let Some(w) = self.writer.lock().unwrap().as_mut() {
            for s in samples {
                if w.write_sample(s).is_err() {
                    self.lost.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }
    }
}

/// Gravação do microfone numa thread própria (o `cpal::Stream` fica nela do início ao fim).
pub(crate) struct Audio {
    shared: Arc<Shared>,
    stop_tx: Sender<()>,
    thread: JoinHandle<anyhow::Result<()>>,
}

impl Audio {
    pub(crate) fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Relaxed);
    }

    pub(crate) fn lost(&self) -> bool {
        self.shared.lost.load(Ordering::Relaxed)
    }

    /// Para a captura, finaliza o cabeçalho do WAV e devolve o `audio_offset_ms`.
    pub(crate) fn stop(self) -> anyhow::Result<Option<i64>> {
        let _ = self.stop_tx.send(());
        self.thread
            .join()
            .map_err(|_| anyhow!("a thread de áudio entrou em pânico"))??;
        if let Some(w) = self.shared.writer.lock().unwrap().take() {
            w.finalize()?;
        }
        Ok(*self.shared.offset.lock().unwrap())
    }
}

pub(crate) fn start(path: PathBuf, t0: Instant) -> anyhow::Result<Audio> {
    let (ready_tx, ready_rx) = channel::<anyhow::Result<Arc<Shared>>>();
    let (stop_tx, stop_rx) = channel::<()>();
    let thread = std::thread::spawn(move || -> anyhow::Result<()> {
        let init = (|| -> anyhow::Result<(cpal::Stream, Arc<Shared>)> {
            let device = cpal::default_host()
                .default_input_device()
                .ok_or_else(|| anyhow!("nenhum microfone encontrado"))?;
            let config = device.default_input_config()?;
            let channels = config.channels() as usize;
            let rate = config.sample_rate().0;
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let shared = Arc::new(Shared {
                writer: Mutex::new(Some(hound::WavWriter::create(&path, spec)?)),
                offset: Mutex::new(None),
                paused: AtomicBool::new(false),
                lost: AtomicBool::new(false),
            });
            let on_error = {
                let shared = shared.clone();
                move |e: cpal::StreamError| {
                    eprintln!("erro de áudio: {e}");
                    shared.lost.store(true, Ordering::Relaxed);
                }
            };
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let s = shared.clone();
                    device.build_input_stream(
                        &config.config(),
                        move |data: &[f32], _: &_| s.on_block(data, channels, rate, t0, f32_sample),
                        on_error,
                        None,
                    )?
                }
                cpal::SampleFormat::I16 => {
                    let s = shared.clone();
                    device.build_input_stream(
                        &config.config(),
                        move |data: &[i16], _: &_| s.on_block(data, channels, rate, t0, i16_sample),
                        on_error,
                        None,
                    )?
                }
                other => bail!("formato de áudio não suportado: {other:?}"),
            };
            stream.play()?;
            Ok((stream, shared))
        })();
        match init {
            Ok((stream, shared)) => {
                let _ = ready_tx.send(Ok(shared));
                let _ = stop_rx.recv();
                drop(stream);
                Ok(())
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                Ok(())
            }
        }
    });
    let shared = ready_rx
        .recv()
        .map_err(|_| anyhow!("a thread de áudio terminou antes de iniciar"))??;
    Ok(Audio {
        shared,
        stop_tx,
        thread,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixdown_averages_channels_and_zeroes_when_paused() {
        let stereo = [1.0f32, -1.0, 0.5, 0.5];
        assert_eq!(mixdown(&stereo, 2, false, f32_sample), vec![0, 16383]);
        assert_eq!(mixdown(&stereo, 2, true, f32_sample), vec![0, 0]);
        let mono = [i16::MAX, i16::MIN];
        assert_eq!(
            mixdown(&mono, 1, false, i16_sample),
            vec![i16::MAX, -i16::MAX]
        );
    }

    #[test]
    fn offset_subtracts_first_block_duration() {
        assert_eq!(first_block_offset_ms(130, 4800, 48_000), 30);
        assert_eq!(first_block_offset_ms(5, 4800, 48_000), -95);
    }
}
