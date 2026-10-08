mod error;
mod generate;
mod images;
mod process;
mod publish;
mod recording;

pub use error::{CommandError, CommandResult};
pub use generate::{generate_manual, improve_manual};
pub use images::{add_operator_image, image_path, republish, set_step_image, MAX_IMAGEM};
pub use process::process_session;
pub use publish::{approve, fetch_published, publish_draft, FetchReport};
pub use recording::{start_recording, stop_recording};
