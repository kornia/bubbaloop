use crate::api::models::inference::InferenceResult;
use crate::cu29::msgs::ImageRgb8Msg;
use crate::pipeline::SERVER_GLOBAL_STATE;
use kornia_yolo::{YoloV8, YoloV8Config};

use cu29::prelude::*;

/// Task that runs inference on an image
pub struct Inference {
    model: YoloV8,
}

impl Freezable for Inference {}

impl<'cl> CuSinkTask<'cl> for Inference {
    type Input = input_msg!('cl, ImageRgb8Msg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        let model = YoloV8::new(YoloV8Config::default())
            .map_err(|e| CuError::new_with_cause("Failed to load YOLOv8 model", e))?;
        Ok(Self { model })
    }

    fn process(&mut self, clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let Some(msg) = input.payload() else {
            return Ok(());
        };

        // run inference of the model
        let bboxes = self
            .model
            .inference(msg)
            .map_err(|e| CuError::new_with_cause("Failed to run inference", e))?;

        log::debug!("bboxes: {:?}", bboxes);

        // Store the result in the global state with write lock
        if let Ok(mut result_store) = SERVER_GLOBAL_STATE.result_store.inference.write() {
            result_store.push_back(InferenceResult {
                timestamp_nanos: clock.now().as_nanos(),
                detections: bboxes,
            });
        }

        Ok(())
    }
}
