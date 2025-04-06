use crate::cu29::msgs::{ImageRgb8Msg, PromptResponseMsg};
use crate::pipeline::SERVER_GLOBAL_STATE;
use cu29::prelude::*;
use kornia_paligemma::{Paligemma, PaligemmaConfig};
use std::sync::Arc;
use std::sync::Mutex;

/// The default prompt to use if no prompt is provided
// NOTE: check the original prompt instructions
// https://ai.google.dev/gemma/docs/paligemma/prompt-system-instructions
const DEFAULT_PROMPT: &str = "cap en";

/// Task that runs inference on an image
pub struct Inference {
    paligemma: Paligemma,
    current_prompt: Arc<Mutex<String>>,
}

impl Freezable for Inference {}

impl<'cl> CuTask<'cl> for Inference {
    type Input = input_msg!('cl, ImageRgb8Msg);
    type Output = output_msg!('cl, PromptResponseMsg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        let paligemma = Paligemma::new(PaligemmaConfig::default())
            .map_err(|e| CuError::new_with_cause("Failed to create Paligemma", e))?;

        Ok(Self {
            paligemma,
            current_prompt: Arc::new(Mutex::new(DEFAULT_PROMPT.to_string())),
        })
    }

    fn process(
        &mut self,
        clock: &RobotClock,
        input: Self::Input,
        output: Self::Output,
    ) -> Result<(), CuError> {
        // check first if we should update the prompt
        if let Ok(prompt) = SERVER_GLOBAL_STATE
            .result_store
            .inference_settings
            .rx
            .lock()
            .unwrap()
            .try_recv()
        {
            log::debug!("Updating prompt to: {}", prompt);
            *self.current_prompt.lock().unwrap() = prompt;
        }

        // check if we have an image and run the inference
        let Some(img) = input.payload() else {
            return Ok(());
        };

        let current_prompt = self.current_prompt.lock().unwrap();

        let response = self
            .paligemma
            .inference(img, &current_prompt, 50, false)
            .map_err(|e| CuError::new_with_cause("Failed to run inference", e))?;

        let response_msg = PromptResponseMsg {
            prompt: current_prompt.clone(),
            response,
        };

        log::debug!("response: {:?}", response_msg);

        output.metadata.tov = clock.now().into();
        output.set_payload(response_msg);

        Ok(())
    }
}
