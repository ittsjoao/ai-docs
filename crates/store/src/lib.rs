//! Adapters de arquivo: a pasta da sessão (`SessionStore`) e, na Task 2, os recortes (`Imaging`).
mod fs_store;

pub use fs_store::{utc_now_rfc3339, FsStore};
