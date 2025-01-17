use cu29::prelude::*;

use crate::cu29::app::COPPER_GLOBAL_STATE;
use crate::cu29::msgs::MeanStdMsg;

pub struct Print;

impl Freezable for Print {}

impl<'cl> CuSinkTask<'cl> for Print {
    type Input = input_msg!('cl, MeanStdMsg);

    fn new(_config: Option<&ComponentConfig>) -> CuResult<Self> {
        Ok(Self {})
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> CuResult<()> {
        if let Some(msg) = input.payload() {
            if let Ok(mut global_state) = COPPER_GLOBAL_STATE.lock() {
                global_state.mean_std_msg = Some(msg.clone());
            }
        }
        Ok(())
    }
}
