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
        // clear the payload of the output message to avoid
        // sending the same text message multiple times even if we do not receive a new image
        output.clear_payload();

        // receive the text message from the rest api
        let Ok(text) = SERVER_GLOBAL_STATE
            .result_store
            .inference
            .query
            .rx
            .lock()
            .unwrap()
            .try_recv()
        else {
            return Ok(());
        };

        // forward the text message to the inference task
        output.set_payload(ChatTextMsg { text });

        Ok(())
    }
}
