use cu29::prelude::*;

use crate::cu29::msgs::MeanStdMsg;

pub struct Print;

impl Freezable for Print {}

impl<'cl> CuSinkTask<'cl> for Print {
    type Input = input_msg!('cl, MeanStdMsg);

    fn new(_config: Option<&ComponentConfig>) -> CuResult<Self> {
        Ok(Self {})
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> CuResult<()> {
        // TODO: figure out how to expose this to the user
        if let Some(msg) = input.payload() {
            println!("mean: {:?}, std: {:?}", msg.mean, msg.std);
        }
        Ok(())
    }
}
