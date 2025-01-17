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

        let mean_std = MeanStdMsg { mean, std };

        output.set_payload(mean_std);

        Ok(())
    }
}
