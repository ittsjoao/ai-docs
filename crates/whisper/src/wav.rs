use std::path::Path;

use anyhow::{bail, Context, Result};

const PCM: u16 = 1;
const EXTENSIBLE: u16 = 0xFFFE;

/// Lê um WAV PCM 16-bit e devolve (taxa, amostras mono em −1..1).
pub(crate) fn read_mono(path: &Path) -> Result<(u32, Vec<f32>)> {
    let bytes = std::fs::read(path).with_context(|| format!("falha ao ler {}", path.display()))?;
    parse_mono(&bytes).with_context(|| format!("WAV inválido: {}", path.display()))
}

/// Tolera o WAV que a gravação deixa quando o app cai antes do `finalize`: tamanho do chunk
/// `data` igual a 0 ou maior que o arquivo vira "até o fim do arquivo" (spike, achado 5).
pub(crate) fn parse_mono(b: &[u8]) -> Result<(u32, Vec<f32>)> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        bail!("cabeçalho RIFF/WAVE ausente");
    }
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let mut fmt: Option<(u16, u32)> = None; // (canais, taxa)
    let mut pos = 12;
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let size = u32_at(pos + 4) as usize;
        let body = pos + 8;
        if id == b"fmt " {
            if body + 16 > b.len() {
                bail!("chunk fmt truncado");
            }
            let format = u16_at(body);
            if format != PCM && format != EXTENSIBLE {
                bail!("formato WAV {format} não suportado; esperado PCM");
            }
            let bits = u16_at(body + 14);
            if bits != 16 {
                bail!("WAV de {bits} bits não suportado; esperado 16");
            }
            fmt = Some((u16_at(body + 2).max(1), u32_at(body + 4)));
        } else if id == b"data" {
            let Some((channels, rate)) = fmt else {
                bail!("chunk data antes do fmt")
            };
            let avail = b.len() - body;
            let len = if size == 0 || size > avail {
                avail
            } else {
                size
            };
            let samples: Vec<f32> = b[body..body + len - len % 2]
                .chunks_exact(2)
                .map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / i16::MAX as f32)
                .collect();
            let mono = samples
                .chunks(channels as usize)
                .map(|f| f.iter().sum::<f32>() / f.len() as f32)
                .collect();
            return Ok((rate, mono));
        }
        pos = body + size + size % 2;
    }
    bail!("chunk data ausente")
}

/// Reamostragem por média de janela: cada amostra de saída é a média das amostras de entrada
/// que ela cobre (filtra o aliasing de 48→16 kHz); ao subir a taxa, repete a amostra mais próxima.
pub(crate) fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || x.is_empty() || from == 0 || to == 0 {
        return x.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = (x.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let a = (i as f64 * ratio) as usize;
            let b = (((i + 1) as f64 * ratio) as usize).clamp(a + 1, x.len());
            x[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// RMS das amostras entre `start_ms` e `end_ms` (0 se a janela estiver vazia ou fora do áudio).
pub(crate) fn rms(samples: &[f32], rate: u32, start_ms: u64, end_ms: u64) -> f32 {
    let idx = |ms: u64| ((ms * rate as u64 / 1000) as usize).min(samples.len());
    let (a, b) = (idx(start_ms), idx(end_ms));
    if b <= a {
        return 0.0;
    }
    (samples[a..b].iter().map(|s| s * s).sum::<f32>() / (b - a) as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_bytes(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cursor, spec).unwrap();
            for s in samples {
                w.write_sample(*s).unwrap();
            }
            w.finalize().unwrap();
        }
        cursor.into_inner()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn reads_finalized_mono() {
        let (rate, x) = parse_mono(&wav_bytes(48_000, 1, &[0, 16_384, -16_384, i16::MAX])).unwrap();
        assert_eq!((rate, x.len()), (48_000, 4));
        assert!(
            close(x[0], 0.0) && close(x[1], 0.5) && close(x[2], -0.5) && close(x[3], 1.0),
            "{x:?}"
        );
    }

    #[test]
    fn stereo_is_averaged_to_mono() {
        let (_, x) = parse_mono(&wav_bytes(44_100, 2, &[16_384, 0, -16_384, -16_384])).unwrap();
        assert_eq!(x.len(), 2);
        assert!(close(x[0], 0.25) && close(x[1], -0.5), "{x:?}");
    }

    #[test]
    fn unfinalized_header_still_yields_the_samples() {
        let mut b = wav_bytes(48_000, 1, &[100, 200, 300, 400]);
        b[4..8].fill(0); // tamanho RIFF
        let data = b.windows(4).position(|w| w == b"data").unwrap();
        b[data + 4..data + 8].fill(0); // tamanho do chunk data, como num crash antes do finalize
        b.push(7); // meio sample no fim
        let (_, x) = parse_mono(&b).unwrap();
        assert_eq!(x.len(), 4);
        b[data + 4..data + 8].copy_from_slice(&1_000_000u32.to_le_bytes()); // maior que o arquivo
        assert_eq!(parse_mono(&b).unwrap().1.len(), 4);
    }

    #[test]
    fn rejects_what_is_not_pcm16_wav() {
        assert!(parse_mono(b"hello").is_err());
        let mut b = wav_bytes(48_000, 1, &[1, 2]);
        let fmt = b.windows(4).position(|w| w == b"fmt ").unwrap();
        b[fmt + 22..fmt + 24].copy_from_slice(&24u16.to_le_bytes()); // bits por amostra
        assert!(parse_mono(&b).unwrap_err().to_string().contains("24 bits"));
    }

    #[test]
    fn resample_averages_windows() {
        assert_eq!(
            resample(&[1.0, 1.0, 1.0, -1.0, -1.0, -1.0], 48_000, 16_000),
            vec![1.0, -1.0]
        );
        let y = resample(&vec![0.3; 48_000], 48_000, 16_000);
        assert_eq!(y.len(), 16_000);
        assert!(y.iter().all(|v| close(*v, 0.3)));
        assert_eq!(resample(&[0.1, 0.2], 16_000, 16_000), vec![0.1, 0.2]);
        assert_eq!(
            resample(&[0.5, -0.5], 8_000, 16_000).len(),
            4,
            "8 kHz sobe repetindo amostras"
        );
    }

    #[test]
    fn rms_of_a_time_window() {
        let x = vec![0.5; 16_000];
        assert!(close(rms(&x, 16_000, 0, 1000), 0.5));
        assert_eq!(rms(&x, 16_000, 900, 900), 0.0);
        assert_eq!(rms(&x, 16_000, 5000, 6000), 0.0, "fora do áudio");
    }
}
