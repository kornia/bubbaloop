use cu29::prelude::*;
use kornia::imgproc;

use crate::cu29::msgs::{ImageRGBU8Msg, MeanStdMsg};

pub struct Imgproc;

impl Freezable for Imgproc {}

impl<'cl> CuTask<'cl> for Imgproc {
    type Input = input_msg!('cl, ImageRGBU8Msg);
    type Output = output_msg!('cl, MeanStdMsg);

    fn new(_config: Option<&ComponentConfig>) -> CuResult<Self>
    where
        Self: Sized,
    {
        Ok(Self {})
    }

    fn process(
        &mut self,
        _clock: &RobotClock,
        input: Self::Input,
        output: Self::Output,
    ) -> CuResult<()> {
        let Some(msg) = input.payload() else {
            return Ok(());
        };

        // TODO: upstream this to kornia to return arrays instead of vectors
        let (mean, std) = imgproc::core::std_mean(&msg.image);

        let mean_array = [mean[0] as f32, mean[1] as f32, mean[2] as f32];
        let std_array = [std[0] as f32, std[1] as f32, std[2] as f32];

        output.set_payload(MeanStdMsg {
            mean: mean_array,
            std: std_array,
        });

        Ok(())
    }
}
