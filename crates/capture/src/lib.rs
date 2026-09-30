//! Captura no Windows: implementa `screenmanual_core::ports::Recorder`.
mod aggregate;
mod audio;
mod dhash;
mod hooks;
mod keys;
mod privacy;
mod quality;
mod recorder;
mod sink;
mod win;

pub use recorder::{WinHandle, WinRecorder};

/// Coordenadas de hook, UIA e xcap em pixels físicos (validado no spike a 100/125/150%).
/// O app Tauri declara isso no manifesto; o exemplo chama esta função.
pub fn set_dpi_awareness() {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}
