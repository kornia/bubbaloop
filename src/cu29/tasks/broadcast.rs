use crate::{
    api::models::{camera::CameraResult, inference::InferenceResult},
    cu29::msgs::{EncodedImage, ImageRgb8Msg, PromptResponseMsg},
    pipeline::SERVER_GLOBAL_STATE,
};
use cu29::prelude::*;
use kornia::io::jpeg::ImageEncoder;
use std::time::Duration;

pub struct Broadcast {
    image_encoder: ImageEncoder,
}

impl Freezable for Broadcast {}

impl<'cl> CuSinkTask<'cl> for Broadcast {
    type Input = input_msg!('cl, ImageRgb8Msg, PromptResponseMsg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self {
            image_encoder: ImageEncoder::new()
                .map_err(|e| CuError::new_with_cause("Failed to create jpeg encoder", e))?,
        })
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let (img_msg, prompt_msg) = input;

        // broadcast the image
        if let Some(img) = img_msg.payload() {
            let encoded_image = self
                .image_encoder
                .encode(img)
                .map_err(|e| CuError::new_with_cause("Failed to encode image", e))?;

            // get the acquisition time of the image
            let acq_time: Duration = match img_msg.metadata.tov {
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
                    image: EncodedImage {
                        data: encoded_image,
                        encoding: "jpeg".to_string(),
                    },
                });
        }

        // broadcast the prompt response
        if let Some(prompt) = prompt_msg.payload() {
            let acq_time: Duration = match prompt_msg.metadata.tov {
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
        }

        Ok(())
    }
}
