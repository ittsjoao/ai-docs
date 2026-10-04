//! Transcrição local com whisper-rs: implementa `screenmanual_core::ports::Transcriber`.
mod transcriber;
mod wav;

pub use transcriber::{model_file, WhisperTranscriber};
