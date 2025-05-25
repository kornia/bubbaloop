use crate::{
    cu29::msgs::{ImageRgb8Msg, PromptResponseMsg},
    pipeline::SERVER_GLOBAL_STATE,
};
use cu29::prelude::*;
use kornia_infernum::{
    engine::{InfernumEngine, InfernumEngineRequest, InfernumEngineResult, InfernumEngineState},
    model::{InfernumModel, InfernumModelRequest, InfernumModelResponse},
};
use kornia_paligemma::{Paligemma, PaligemmaConfig, PaligemmaError};

/// The default prompt to use if no prompt is provided
// NOTE: check the original prompt instructions
// https://ai.google.dev/gemma/docs/paligemma/prompt-system-instructions
const DEFAULT_PROMPT: &str = "cap en\n";

/// Task that runs inference on an image
pub struct Inference {
    current_prompt: String,
    engine: InfernumEngine<PaligemmaModel>,
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

        let engine = InfernumEngine::new(PaligemmaModel(paligemma));

        Ok(Self {
            current_prompt: DEFAULT_PROMPT.to_string(),
            engine,
        })
    }

    fn process(
        &mut self,
        _clock: &RobotClock,
        input: Self::Input,
        output: Self::Output,
    ) -> Result<(), CuError> {
        // clear the output payload to avoid any previous payload to be forwarded
        output.clear_payload();

        // check first if we should update the prompt
        if let Ok(prompt) = SERVER_GLOBAL_STATE
            .result_store
            .inference_settings
            .rx
            .lock()
            .expect("Failed to lock inference settings")
            .try_recv()
        {
            log::debug!("Updating prompt to: {}", prompt);
            self.current_prompt = prompt;
        }

        // check if we are already processing an inference to not block the main thread
        if self.engine.state() == InfernumEngineState::Processing {
            return Ok(());
        }

        // check first if we have a response from the previous inference
        if let InfernumEngineResult::Success(response) = self.engine.try_poll_response() {
            log::debug!(
                "Received response from inference thread for channel: {} -- prompt: {} -- response: {}",
                response.id,
                response.prompt,
                response.response
            );

            output.set_payload(PromptResponseMsg {
                stamp_ns: response.duration.as_nanos() as u64,
                channel_id: response.id,
                prompt: response.prompt,
                response: response.response,
            });
        }

        // check if we have a new image and schedule the inference
        let Some(img) = input.payload() else {
            return Ok(());
        };

        // send the request to the thread to schedule the inference
        self.engine.schedule_inference(InfernumEngineRequest {
            id: img.channel_id,
            prompt: self.current_prompt.clone(),
            image: img.image.clone(),
        });

        Ok(())
    }
}

/// Model that uses Paligemma to run inference
struct PaligemmaModel(Paligemma);

impl InfernumModel for PaligemmaModel {
    type Error = PaligemmaError;

    fn run(&mut self, request: InfernumModelRequest) -> Result<InfernumModelResponse, Self::Error> {
        let response = self
            .0
            .inference(&request.image, &request.prompt, 50, false)?;

        Ok(InfernumModelResponse { response })
    }
}
