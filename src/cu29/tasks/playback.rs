use crate::{
    api::models::recording::RecordingCommand,
    cu29::msgs::{EncodedImage, ImageRgb8Msg},
    pipeline::SERVER_GLOBAL_STATE,
};
use cu29::prelude::*;
use std::path::{Path, PathBuf};

enum PlaybackState {
    Stopped,
    Playing(rerun::RecordingStream),
}

pub struct PlaybackOne {
    state: PlaybackState,
    path: PathBuf,
}

impl Freezable for PlaybackOne {}

impl<'cl> CuSinkTask<'cl> for PlaybackOne {
    type Input = input_msg!('cl, ImageRgb8Msg);

    fn new(config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        let config = config.expect("config is required");
        let log_path = config
            .get::<String>("log_path")
            .expect("log_path is required");

        let rec = rerun::RecordingStreamBuilder::new("rerun_logger")
            .spawn()
            .map_err(|e| CuError::new_with_cause("Failed to spawn rerun stream", e))?;

        rec.log_file_from_path(&log_path, None, true)
            .map_err(|e| CuError::new_with_cause("Failed to log file", e))?;

        Ok(Self {
            state: PlaybackState::Stopped,
            path: PathBuf::from(log_path),
        })
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        Ok(())
    }
}
