use crate::{
    api::models::camera::CameraResult,
    cu29::msgs::{ChatTextMsg, EncodedImage, ImageRgb8Msg},
    pipeline::SERVER_GLOBAL_STATE,
};
use cu29::prelude::*;
use kornia::io::jpegturbo::JpegTurboEncoder;

pub struct Broadcast {
    jpeg_encoder: JpegTurboEncoder,
}

impl Freezable for Broadcast {}

impl<'cl> CuSinkTask<'cl> for Broadcast {
    type Input = input_msg!('cl, ImageRgb8Msg, ChatTextMsg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self {
            jpeg_encoder: JpegTurboEncoder::new()
                .map_err(|e| CuError::new_with_cause("Failed to create jpeg encoder", e))?,
        })
    }

    fn process(&mut self, clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let (img_msg, inference_msg) = input;

        if let Some(img) = img_msg.payload() {
            // encode the image to jpeg before broadcasting
            let encoded_image = self
                .jpeg_encoder
                .encode_rgb8(img)
                .map_err(|e| CuError::new_with_cause("Failed to encode image", e))?;

            // send the camera image to the global state
            SERVER_GLOBAL_STATE
                .result_store
                .image
                .tx
                .send(CameraResult {
                    // TODO: not clone and send the reference
                    timestamp_nanos: clock.now().as_nanos(),
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
        }

        if let Some(chat_result) = inference_msg.payload() {
            // send the chat result to the global state
            SERVER_GLOBAL_STATE
                .result_store
                .inference
                .result
                .tx
                .send(chat_result.text.clone())
                .map_err(|e| CuError::new_with_cause("Failed to send chat result", e))?;
        }

        Ok(())
    }
}
