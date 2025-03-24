use crate::api::models::inference::InferenceResult;
use crate::cu29::msgs::ImageRgb8Msg;
use crate::pipeline::SERVER_GLOBAL_STATE;

use cu29::prelude::*;

use kornia::imgproc;

/// NOTE: placeholder to test the inference pipeline
pub struct Model;

impl Freezable for Model {}

impl<'cl> CuSinkTask<'cl> for Model {
    type Input = input_msg!('cl, ImageRgb8Msg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self)
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let Some(msg) = input.payload() else {
            return Ok(());
        };

        // TODO: implement object detection
        // compute the mean and std of the image
        let (std, mean) = imgproc::core::std_mean(msg);

        // store the result in the global state
        if let Ok(mut result_store) = SERVER_GLOBAL_STATE.result_store.0.lock() {
            let result = InferenceResult::new_mean_std(
                [mean[0], mean[1], mean[2]],
                [std[0], std[1], std[2]],
            );
            result_store.insert("inference".to_string(), result);
        }
        Ok(())
    }
}
