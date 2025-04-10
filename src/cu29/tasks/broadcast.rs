use crate::{
    api::models::{camera::CameraResult, inference::InferenceResult},
    cu29::msgs::{EncodedImage, PromptResponseMsg},
    pipeline::SERVER_GLOBAL_STATE,
};
use cu29::prelude::*;
use std::time::Duration;

pub struct ImageBroadcast;

impl Freezable for ImageBroadcast {}

impl<'cl> CuSinkTask<'cl> for ImageBroadcast {
    type Input = input_msg!('cl, EncodedImage);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self {})
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        // broadcast the image
        if let Some(img) = input.payload() {
            let acq_time: Duration = match input.metadata.tov {
                Tov::Time(time) => time.into(),
                _ => Duration::from_secs(0),
            };

            // send the camera image to the global state
            let _ = SERVER_GLOBAL_STATE
                .result_store
                .image
                .tx
                .send(CameraResult {
                    timestamp_nanos: acq_time.as_nanos() as u64,
                    image: img.clone(),
                });
        }

        Ok(())
    }
}

pub struct InferenceBroadcast;

impl Freezable for InferenceBroadcast {}

impl<'cl> CuSinkTask<'cl> for InferenceBroadcast {
    type Input = input_msg!('cl, PromptResponseMsg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError> {
        Ok(Self {})
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let Some(prompt) = input.payload() else {
            return Ok(());
        };

        let acq_time: Duration = match input.metadata.tov {
            Tov::Time(time) => time.into(),
            _ => Duration::from_secs(0),
        };

        let _ = SERVER_GLOBAL_STATE
            .result_store
            .inference
            .tx
            .send(InferenceResult {
                timestamp_nanos: acq_time.as_nanos() as u64,
                prompt: prompt.prompt.clone(),
                response: prompt.response.clone(),
            });

        Ok(())
    }
}
