use crate::cu29::msgs::{ChatTextMsg, ImageRgb8Msg};
use cu29::prelude::*;

/// Task that runs inference on an image
pub struct Inference;

impl Freezable for Inference {}

impl<'cl> CuTask<'cl> for Inference {
    type Input = input_msg!('cl, ImageRgb8Msg, ChatTextMsg);
    type Output = output_msg!('cl, ChatTextMsg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self)
    }

    fn process(
        &mut self,
        _clock: &RobotClock,
        input: Self::Input,
        output: Self::Output,
    ) -> Result<(), CuError> {
        let (img_msg, text_msg) = input;
        // clear the payload of the output message to avoid
        // sending the same text message multiple times even if we do not receive a new image
        output.clear_payload();

        let Some(img) = img_msg.payload() else {
            return Ok(());
        };

        let Some(text) = text_msg.payload() else {
            return Ok(());
        };

        // run inference of the model
        let response = dummy_inference(img, text)
            .map_err(|e| CuError::new_with_cause("Failed to run inference", e))?;

        let response_msg = ChatTextMsg { text: response };
        log::debug!("response: {:?}", response_msg);

        output.set_payload(response_msg);

        Ok(())
    }
}

fn dummy_inference(img: &ImageRgb8Msg, text: &ChatTextMsg) -> Result<String, CuError> {
    Ok(format!(
        "dummy inference: {} -- image size: {:?}",
        text.text,
        img.size()
    ))
}
