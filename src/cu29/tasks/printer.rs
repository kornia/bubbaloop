use crate::cu29::msgs::ImageRgb8Msg;
use cu29::prelude::*;

pub struct Printer {
    counter: usize,
}

impl Freezable for Printer {}

impl<'cl> CuSinkTask<'cl> for Printer {
    type Input = input_msg!('cl, ImageRgb8Msg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self { counter: 0 })
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let Some(msg) = input.payload() else {
            return Ok(());
        };
        log::debug!("Received image {} : {:?}", self.counter, msg.0.size());
        self.counter += 1;
        Ok(())
    }
}
