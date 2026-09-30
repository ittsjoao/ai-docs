//! Modelo real (não roda no CI). Usa o modelo e a narração do spike:
//! Rode da raiz do repositório com caminhos absolutos: o binário de teste roda com o
//! diretório do crate como cwd, então caminhos relativos não são encontrados.
//! $env:SCREENMANUAL_MODEL = "$PWD\spike_out\models\ggml-large-v3-turbo-q5_0.bin"
//! $env:SCREENMANUAL_WAV = "$PWD\spike_out\narracao2.wav"
//! cargo test -p screenmanual-whisper --release -- --ignored --nocapture
use std::path::{Path, PathBuf};

use screenmanual_core::ports::Transcriber;
use screenmanual_whisper::WhisperTranscriber;

#[test]
#[ignore = "requer SCREENMANUAL_MODEL e SCREENMANUAL_WAV"]
fn transcribes_the_spike_narration() {
    let model = std::env::var("SCREENMANUAL_MODEL").expect("defina SCREENMANUAL_MODEL");
    let wav = std::env::var("SCREENMANUAL_WAV").expect("defina SCREENMANUAL_WAV");
    let started = std::time::Instant::now();
    let segs = WhisperTranscriber::new(PathBuf::from(model))
        .transcribe(Path::new(&wav), "")
        .unwrap();
    for s in &segs {
        println!("[{:>6}–{:>6}] {}", s.start, s.end, s.text);
    }
    println!(
        "{} segmentos em {:.1}s",
        segs.len(),
        started.elapsed().as_secs_f32()
    );
    assert!(!segs.is_empty());
    assert!(
        segs.windows(2).all(|w| w[0].start <= w[1].start),
        "segmentos em ordem"
    );
    assert!(segs
        .iter()
        .all(|s| s.start <= s.end && !s.text.trim().is_empty()));
    assert!(
        segs.iter().any(|s| !s.words.is_empty()),
        "palavras com tempo"
    );
}
