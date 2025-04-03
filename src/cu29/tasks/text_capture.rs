use crate::cu29::msgs::ChatTextMsg;
use crate::pipeline::SERVER_GLOBAL_STATE;
use cu29::prelude::*;

pub struct TextCapture;

impl Freezable for TextCapture {}

impl<'cl> CuSrcTask<'cl> for TextCapture {
    type Output = output_msg!('cl, ChatTextMsg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self)
    }

    fn process(&mut self, _clock: &RobotClock, output: Self::Output) -> Result<(), CuError> {
        let Ok(text) = SERVER_GLOBAL_STATE
            .result_store
            .inference
            .query
            .rx
            .lock()
            .unwrap()
            .try_recv()
        else {
            log::debug!("no text");
            return Ok(());
        };

        log::debug!("text: {:?}", text);

        output.set_payload(ChatTextMsg { text });

        Ok(())
    }
}
