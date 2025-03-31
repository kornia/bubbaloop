use crate::{
    api::models::{camera::CameraResult, inference::InferenceResult},
    cu29::msgs::{ImageRgb8Msg, InferenceResultMsg},
    pipeline::SERVER_GLOBAL_STATE,
};
use cu29::prelude::*;
use kornia_yolo::BoundingBox;

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
            if SERVER_GLOBAL_STATE
                .result_store
                .image
                .tx
                .send(CameraResult {
                    // TODO: not clone and send the reference
                    timestamp_nanos: clock.now().as_nanos(),
                    // TODO: not clone and send the reference
                    image: img.clone(),
                })
                .is_ok()
            {}
        }

        if let Some(inference_result) = inference_msg.payload() {
            // send the inference result to the global state
            if SERVER_GLOBAL_STATE
                .result_store
                .inference
                .tx
                .send(InferenceResult {
                    timestamp_nanos: clock.now().as_nanos(),
                    detections: inference_result
                        .0
                        .iter()
                        .map(|b| b.0)
                        .collect::<Vec<BoundingBox>>(),
                })
                .is_ok()
            {}
        }

        Ok(())
    }
}
