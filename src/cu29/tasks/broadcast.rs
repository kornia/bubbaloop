use crate::{
    api::models::inference::InferenceResult,
    cu29::msgs::{EncodedImage, PromptResponseMsg},
    pipeline::SERVER_GLOBAL_STATE,
};
use cu29::prelude::*;

pub struct ImageBroadcast1;

impl Freezable for ImageBroadcast1 {}

impl<'cl> CuSinkTask<'cl> for ImageBroadcast1 {
    type Input = input_msg!('cl, EncodedImage);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self {})
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        // broadcast the image
        if let Some(msg) = input.payload() {
            // send the camera image to the global state
            let _ = SERVER_GLOBAL_STATE.result_store.images[msg.channel_id as usize]
                .tx
                .send(msg.clone());
        }
        Ok(())
    }
}

pub struct ImageBroadcast2;

impl Freezable for ImageBroadcast2 {}

impl<'cl> CuSinkTask<'cl> for ImageBroadcast2 {
    type Input = input_msg!('cl, EncodedImage, EncodedImage);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError> {
        Ok(Self {})
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let (msg1, msg2) = input;
        if let Some(msg1) = msg1.payload() {
            let _ = SERVER_GLOBAL_STATE.result_store.images[msg1.channel_id as usize]
                .tx
                .send(msg1.clone());
        }
        if let Some(msg2) = msg2.payload() {
            let _ = SERVER_GLOBAL_STATE.result_store.images[msg2.channel_id as usize]
                .tx
                .send(msg2.clone());
        }
        Ok(())
    }
}

pub struct InferenceBroadcast1;

impl Freezable for InferenceBroadcast1 {}

impl<'cl> CuSinkTask<'cl> for InferenceBroadcast1 {
    type Input = input_msg!('cl, PromptResponseMsg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError> {
        Ok(Self {})
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let Some(prompt) = input.payload() else {
            return Ok(());
        };

        let _ = SERVER_GLOBAL_STATE.result_store.inference[prompt.channel_id as usize]
            .tx
            .send(InferenceResult {
                stamp_ns: prompt.stamp_ns,
                channel_id: prompt.channel_id,
                prompt: prompt.prompt.clone(),
                response: prompt.response.clone(),
            });

        Ok(())
    }
}
