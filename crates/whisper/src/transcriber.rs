use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail};
use screenmanual_core::domain::{Segment, Word};
use screenmanual_core::ports::{PortResult, Transcriber};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::wav;

const RATE: u32 = 16_000;
/// Menos que meio segundo de áudio não tem o que transcrever.
const MIN_SAMPLES: usize = RATE as usize / 2;

/// Segmento como o whisper devolve: tempos em centésimos de segundo e tokens (bytes, t0, t1).
pub(crate) struct RawSegment {
    pub start_cs: i64,
    pub end_cs: i64,
    pub text: String,
    pub tokens: Vec<(Vec<u8>, i64, i64)>,
}

pub struct WhisperTranscriber {
    pub model: PathBuf,
    /// Spike: `min(8, núcleos lógicos)`; no i5-1235U, 11 threads ficou pior que 8.
    pub threads: usize,
    /// Spike (achado 4): o initial_prompt pode gerar lixo; fica desligado por padrão.
    pub use_prompt: bool,
    /// Segmento com RMS abaixo disso é descartado (spec §6.1.5). Calibrar com sessões reais.
    pub min_rms: f32,
}

impl WhisperTranscriber {
    pub fn new(model: PathBuf) -> Self {
        let logical = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self {
            model,
            threads: logical.min(8),
            use_prompt: false,
            min_rms: 0.005,
        }
    }
}

fn cs_to_ms(cs: i64) -> u64 {
    cs.max(0) as u64 * 10
}

/// Junta tokens em palavras: uma palavra nova começa em token com espaço inicial (spec §6.1.4).
/// Acumula bytes e decodifica uma vez por palavra: um token BPE pode trazer só parte de um
/// caractere UTF-8 multibyte.
pub(crate) fn words(tokens: &[(Vec<u8>, i64, i64)]) -> Vec<Word> {
    let mut raw: Vec<(Vec<u8>, u64, u64)> = Vec::new();
    for (bytes, t0, t1) in tokens {
        match raw.last_mut() {
            Some(w) if bytes.first() != Some(&b' ') => {
                w.0.extend_from_slice(bytes);
                w.2 = cs_to_ms(*t1);
            }
            _ => raw.push((bytes.clone(), cs_to_ms(*t0), cs_to_ms(*t1))),
        }
    }
    raw.into_iter()
        .map(|(bytes, s, e)| Word {
            w: String::from_utf8_lossy(&bytes).trim().to_string(),
            s,
            e,
        })
        .filter(|w| !w.w.is_empty())
        .collect()
}

/// Converte para ms, descarta segmento vazio ou em silêncio (RMS abaixo de `min_rms`).
pub(crate) fn to_segments(raw: Vec<RawSegment>, samples: &[f32], min_rms: f32) -> Vec<Segment> {
    raw.into_iter()
        .filter_map(|r| {
            let (start, end) = (cs_to_ms(r.start_cs), cs_to_ms(r.end_cs));
            let text = r.text.trim().to_string();
            if text.is_empty() || wav::rms(samples, RATE, start, end) < min_rms {
                return None;
            }
            Some(Segment {
                start,
                end,
                text,
                words: words(&r.tokens),
            })
        })
        .collect()
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(&self, audio: &Path, prompt: &str) -> PortResult<Vec<Segment>> {
        let (rate, mono) = wav::read_mono(audio)?;
        if rate == 0 {
            bail!("WAV com taxa de amostragem 0: {}", audio.display());
        }
        let samples = wav::resample(&mono, rate, RATE);
        if samples.len() < MIN_SAMPLES {
            return Ok(vec![]);
        }
        if !self.model.is_file() {
            bail!(
                "modelo do whisper não encontrado em {}",
                self.model.display()
            );
        }
        whisper_rs::install_logging_hooks(); // silencia o log do whisper.cpp no stderr
        let ctx = WhisperContext::new_with_params(
            &*self.model.to_string_lossy(),
            WhisperContextParameters::default(),
        )
        .map_err(|e| {
            anyhow!(
                "falha ao carregar o modelo do whisper ({}): {e}",
                self.model.display()
            )
        })?;
        let mut state = ctx.create_state()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("pt"));
        params.set_token_timestamps(true);
        params.set_n_threads(self.threads as i32);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_print_special(false);
        if self.use_prompt && !prompt.is_empty() {
            params.set_initial_prompt(prompt);
        }
        state.full(params, &samples)?;
        let eot = ctx.token_eot();
        let raw = (0..state.full_n_segments())
            .filter_map(|i| state.get_segment(i))
            .map(|seg| RawSegment {
                start_cs: seg.start_timestamp(),
                end_cs: seg.end_timestamp(),
                text: seg
                    .to_str_lossy()
                    .map(|s| s.into_owned())
                    .unwrap_or_default(),
                tokens: (0..seg.n_tokens())
                    .filter_map(|j| seg.get_token(j))
                    .filter(|tk| tk.token_id() < eot) // tokens especiais ([_BEG_], timestamps…)
                    .map(|tk| {
                        let d = tk.token_data();
                        (
                            tk.to_bytes().map(|b| b.to_vec()).unwrap_or_default(),
                            d.t0,
                            d.t1,
                        )
                    })
                    .collect(),
            })
            .collect();
        Ok(to_segments(raw, &samples, self.min_rms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(text: &str, t0: i64, t1: i64) -> (Vec<u8>, i64, i64) {
        (text.as_bytes().to_vec(), t0, t1)
    }

    #[test]
    fn tokens_are_joined_into_words_at_leading_spaces() {
        let w = words(&[
            tok(" Clique", 0, 50),
            tok(" em", 50, 60),
            tok(" No", 60, 80),
            tok("va", 80, 90),
            tok(",", 90, 91),
            tok(" ", 91, 92),
        ]);
        let got: Vec<(&str, u64, u64)> = w.iter().map(|w| (w.w.as_str(), w.s, w.e)).collect();
        assert_eq!(
            got,
            vec![("Clique", 0, 500), ("em", 500, 600), ("Nova,", 600, 910)]
        );
    }

    #[test]
    fn multibyte_char_split_across_tokens_is_not_corrupted() {
        let w = words(&[
            (b" a\xC3".to_vec(), 0, 10),
            (b"\xA7\xC3\xA3o".to_vec(), 10, 20),
        ]);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].w, "ação");
        assert!(!w[0].w.contains('\u{FFFD}'));
    }

    #[test]
    fn silent_and_empty_segments_are_dropped() {
        let mut samples = vec![0.0; 16_000];
        samples.extend(vec![0.5; 16_000]);
        let raw = vec![
            RawSegment {
                start_cs: 0,
                end_cs: 100,
                text: " Legenda inventada no silêncio".into(),
                tokens: vec![],
            },
            RawSegment {
                start_cs: 100,
                end_cs: 200,
                text: " Clique em salvar. ".into(),
                tokens: vec![tok(" Clique", 100, 150)],
            },
            RawSegment {
                start_cs: 150,
                end_cs: 190,
                text: "   ".into(),
                tokens: vec![],
            },
        ];
        let segs = to_segments(raw, &samples, 0.005);
        assert_eq!(segs.len(), 1);
        assert_eq!(
            (segs[0].start, segs[0].end, segs[0].text.as_str()),
            (1000, 2000, "Clique em salvar.")
        );
        assert_eq!(segs[0].words[0].w, "Clique");
    }

    #[test]
    fn defaults_follow_the_spike() {
        let t = WhisperTranscriber::new(PathBuf::from("modelo.bin"));
        assert!(t.threads >= 1 && t.threads <= 8);
        assert!(!t.use_prompt);
    }
}
