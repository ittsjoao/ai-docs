//! Adapters de arquivo: a pasta da sessão (`SessionStore`) e os recortes de imagem (`Imaging`).
mod fs_store;
mod imaging;

pub use fs_store::{session_path, utc_now_rfc3339, FsStore};
pub use imaging::{redact, ImageCrops};
