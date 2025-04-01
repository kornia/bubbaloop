use crate::cu29::msgs::{BoundingBoxMsg, ImageRgb8Msg, InferenceResultMsg};
use cu29::prelude::*;

/// Task that runs inference on an image
pub struct Inference;

impl Freezable for Inference {}

impl<'cl> CuTask<'cl> for Inference {
    type Input = input_msg!('cl, ImageRgb8Msg);
    type Output = output_msg!('cl, InferenceResultMsg);
    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self)
    }

    fn process(
        &mut self,
        clock: &RobotClock,
        input: Self::Input,
        output: Self::Output,
    ) -> Result<(), CuError> {
        let Some(msg) = input.payload() else {
            return Ok(());
        };

        // run inference of the model
        let bboxes = dummy_inference(msg)
            .map_err(|e| CuError::new_with_cause("Failed to run inference", e))?;

        log::debug!("bboxes: {:?}", bboxes);

        output.set_payload(InferenceResultMsg {
            timestamp_nanos: clock.now().as_nanos(),
            detections: bboxes,
        });

        Ok(())
    }
}

fn dummy_inference(_img: &ImageRgb8Msg) -> Result<Vec<BoundingBoxMsg>, CuError> {
    Ok(vec![])
}
