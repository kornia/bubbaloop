use crate::{
    api::models::recording::RecordingCommand,
    cu29::msgs::{EncodedImage, ImageRgb8Msg},
    pipeline::SERVER_GLOBAL_STATE,
};
use cu29::prelude::*;
use std::path::PathBuf;

enum RecorderState {
    Stopped,
    Recording(rerun::RecordingStream),
}

pub struct RecorderOne {
    state: RecorderState,
    path: PathBuf,
}

impl Freezable for RecorderOne {}

impl<'cl> CuSinkTask<'cl> for RecorderOne {
    type Input = input_msg!('cl, EncodedImage);

    fn new(config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        let config = config.expect("config is required");
        let path = config.get::<String>("path").expect("path is required");

        Ok(Self {
            state: RecorderState::Stopped,
            path: PathBuf::from(path),
        })
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        // check if we should start or stop recording
        let maybe_command = SERVER_GLOBAL_STATE
            .result_store
            .recording
            .rx
            .lock()
            .expect("Failed to lock recording")
            .try_recv();

        match &mut self.state {
            RecorderState::Stopped => {
                if let Ok(RecordingCommand::Start) = maybe_command {
                    let rec_path = {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                        self.path.clone().join(format!("{}.rrd", timestamp))
                    };

                    let rec = rerun::RecordingStreamBuilder::new("rerun_logger")
                        .save(&rec_path)
                        .map_err(|e| CuError::new_with_cause("Failed to spawn rerun stream", e))?;

                    self.state = RecorderState::Recording(rec);
                    log::info!("Started recording to {}", rec_path.display());
                }
            }
            RecorderState::Recording(rec) => {
                if let Ok(RecordingCommand::Stop) = maybe_command {
                    rec.flush_blocking();
                    self.state = RecorderState::Stopped;
                    log::info!("Stopped recording");
                    return Ok(());
                } else if let Some(image) = input.payload() {
                    log_image_encoded(rec, &format!("/cam/{}", image.channel_id), image)?;
                }
            }
        }

        Ok(())
    }
}

pub struct RecorderTwo {
    state: RecorderState,
    path: PathBuf,
}

impl Freezable for RecorderTwo {}

impl<'cl> CuSinkTask<'cl> for RecorderTwo {
    type Input = input_msg!('cl, EncodedImage, EncodedImage);

    fn new(config: Option<&ComponentConfig>) -> Result<Self, CuError> {
        let config = config.expect("config is required");
        let path = config.get::<String>("path").expect("path is required");

        Ok(Self {
            state: RecorderState::Stopped,
            path: PathBuf::from(path),
        })
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let maybe_command = SERVER_GLOBAL_STATE
            .result_store
            .recording
            .rx
            .lock()
            .expect("Failed to lock recording")
            .try_recv();

        match &mut self.state {
            RecorderState::Stopped => {
                if let Ok(RecordingCommand::Start) = maybe_command {
                    let rec_path = {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                        self.path.clone().join(format!("{}.rrd", timestamp))
                    };

                    let rec = rerun::RecordingStreamBuilder::new("rerun_logger_two_images")
                        .save(&rec_path)
                        .map_err(|e| CuError::new_with_cause("Failed to spawn rerun stream", e))?;

                    self.state = RecorderState::Recording(rec);
                    log::info!("Started recording to {}", rec_path.display());
                }
            }
            RecorderState::Recording(rec) => {
                if let Ok(RecordingCommand::Stop) = maybe_command {
                    rec.flush_blocking();
                    self.state = RecorderState::Stopped;
                    log::info!("Stopped recording");
                    return Ok(());
                } else {
                    let (msg1, msg2) = input;
                    if let (Some(image1), Some(image2)) = (msg1.payload(), msg2.payload()) {
                        log_image_encoded(rec, &format!("/cam/{}", image1.channel_id), image1)?;
                        log_image_encoded(rec, &format!("/cam/{}", image2.channel_id), image2)?;
                    }
                }
            }
        }

        Ok(())
    }
}

fn _log_image_rgb8(
    rec: &rerun::RecordingStream,
    name: &str,
    msg: &ImageRgb8Msg,
) -> Result<(), CuError> {
    rec.log(
        name,
        &rerun::Image::from_elements(
            msg.image.as_slice(),
            msg.image.size().into(),
            rerun::ColorModel::RGB,
        ),
    )
    .map_err(|e| CuError::new_with_cause("Failed to log image", e))?;
    Ok(())
}

fn log_image_encoded(
    rec: &rerun::RecordingStream,
    name: &str,
    msg: &EncodedImage,
) -> Result<(), CuError> {
    rec.log(
        name,
        &rerun::EncodedImage::from_file_contents(msg.data.clone()),
    )
    .map_err(|e| CuError::new_with_cause("Failed to log image", e))?;
    Ok(())
}
