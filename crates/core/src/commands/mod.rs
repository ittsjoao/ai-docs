mod error;
mod recording;

pub use error::{CommandError, CommandResult};
pub use recording::{start_recording, stop_recording};
