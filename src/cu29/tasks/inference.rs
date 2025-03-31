use crate::cu29::msgs::{BoundingBoxMsg, ImageRgb8Msg, InferenceResultMsg};
use cu29::prelude::*;
use kornia_yolo::{YoloV8, YoloV8Config};

/// Task that runs inference on an image
pub struct Inference {
    model: YoloV8,
}

impl Freezable for Inference {}

impl<'cl> CuTask<'cl> for Inference {
    type Input = input_msg!('cl, ImageRgb8Msg);
    type Output = output_msg!('cl, InferenceResultMsg);
    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        let model = YoloV8::new(YoloV8Config::default())
            .map_err(|e| CuError::new_with_cause("Failed to load YOLOv8 model", e))?;
        Ok(Self { model })
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
        let bboxes = self
            .model
            .inference(msg)
            .map_err(|e| CuError::new_with_cause("Failed to run inference", e))?;

        log::debug!("bboxes: {:?}", bboxes);

        output.set_payload(InferenceResultMsg(
            bboxes.into_iter().map(BoundingBoxMsg).collect(),
        ));
        output.metadata.tov = clock.now().into();

        Ok(())
    }
}
