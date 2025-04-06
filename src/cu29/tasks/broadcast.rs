use std::time::Duration;

use crate::{
    api::models::{camera::CameraResult, inference::InferenceResult},
    cu29::msgs::{EncodedImage, ImageRgb8Msg, PromptResponseMsg},
    pipeline::SERVER_GLOBAL_STATE,
};
use cu29::prelude::*;
use kornia::io::jpegturbo::JpegTurboEncoder;

pub struct BroadcastImage {
    jpeg_encoder: JpegTurboEncoder,
}

impl Freezable for BroadcastImage {}

impl<'cl> CuSinkTask<'cl> for BroadcastImage {
    type Input = input_msg!('cl, ImageRgb8Msg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self {
            jpeg_encoder: JpegTurboEncoder::new()
                .map_err(|e| CuError::new_with_cause("Failed to create jpeg encoder", e))?,
        })
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let Some(img_msg) = input.payload() else {
            return Ok(());
        };

        // encode the image to jpeg before broadcasting
        let encoded_image = self
            .jpeg_encoder
            .encode_rgb8(img_msg)
            .map_err(|e| CuError::new_with_cause("Failed to encode image", e))?;

        // get the acquisition time of the image
        let acq_time: Duration = match input.metadata.tov {
            Tov::Time(time) => time.into(),
            _ => Duration::from_secs(0),
        };

        // send the camera image to the global state
        SERVER_GLOBAL_STATE
            .result_store
            .image
            .tx
            .send(CameraResult {
                // TODO: not clone and send the reference
                timestamp_nanos: acq_time.as_nanos() as u64,
                // TODO: not clone and send the reference
                image: EncodedImage {
                    data: encoded_image,
                    encoding: "jpeg".to_string(),
                },
            })
            .map_err(|e| {
                if matches!(e, tokio::sync::broadcast::error::SendError(_)) {
                    Ok(())
                } else {
                    Err(CuError::new_with_cause("Failed to send camera image", e))
                }
            })
            .ok();

        Ok(())
    }
}

pub struct BroadcastChat;

impl Freezable for BroadcastChat {}

impl<'cl> CuSinkTask<'cl> for BroadcastChat {
    type Input = input_msg!('cl, PromptResponseMsg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self {})
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let Some(chat_msg) = input.payload() else {
            return Ok(());
        };

        // get the timestamp of the inference result
        let acq_time: Duration = match input.metadata.tov {
            Tov::Time(time) => time.into(),
            _ => Duration::from_secs(0),
        };

        // send the chat result to the global state
        SERVER_GLOBAL_STATE
            .result_store
            .inference
            .tx
            .send(InferenceResult {
                //timestamp_nanos: clock.now().as_nanos(),
                timestamp_nanos: acq_time.as_nanos() as u64,
                prompt: chat_msg.prompt.clone(),
                response: chat_msg.response.clone(),
            })
            .map_err(|e| {
                if matches!(e, tokio::sync::broadcast::error::SendError(_)) {
                    Ok(())
                } else {
                    Err(CuError::new_with_cause("Failed to send chat result", e))
                }
            })
            .ok();

        Ok(())
    }
}
