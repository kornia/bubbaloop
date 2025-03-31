use crate::{
    api::models::camera::CameraResult,
    cu29::msgs::{ImageRgb8Msg, InferenceResultMsg},
    pipeline::SERVER_GLOBAL_STATE,
};
use cu29::prelude::*;

pub struct Broadcast;

impl Freezable for Broadcast {}

impl<'cl> CuSinkTask<'cl> for Broadcast {
    type Input = input_msg!('cl, ImageRgb8Msg, InferenceResultMsg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self)
    }

    fn process(&mut self, clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let (img_msg, inference_msg) = input;

        if let Some(img) = img_msg.payload() {
            // send the camera image to the global state
            SERVER_GLOBAL_STATE
                .result_store
                .image
                .tx
                .send(CameraResult {
                    // TODO: not clone and send the reference
                    timestamp_nanos: clock.now().as_nanos(),
                    // TODO: not clone and send the reference
                    image: img.clone(),
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

        if let Some(inference_result) = inference_msg.payload() {
            // send the inference result to the global state
            SERVER_GLOBAL_STATE
                .result_store
                .inference
                .tx
                .send(inference_result.clone())
                .map_err(|e| {
                    if matches!(e, tokio::sync::broadcast::error::SendError(_)) {
                        Ok(())
                    } else {
                        Err(CuError::new_with_cause(
                            "Failed to send inference result",
                            e,
                        ))
                    }
                })
                .ok();
        }

        Ok(())
    }
}
