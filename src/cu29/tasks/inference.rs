use crate::cu29::msgs::{ImageRgb8Msg, PromptResponseMsg};
use crate::pipeline::SERVER_GLOBAL_STATE;
use cu29::prelude::*;
use kornia_paligemma::{Paligemma, PaligemmaConfig, PaligemmaError};
use std::{
    sync::{
        atomic::AtomicBool,
        mpsc::{Receiver, Sender},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

/// The default prompt to use if no prompt is provided
// NOTE: check the original prompt instructions
// https://ai.google.dev/gemma/docs/paligemma/prompt-system-instructions
const DEFAULT_PROMPT: &str = "cap en";

/// Task that runs inference on an image
pub struct Inference {
    current_prompt: String,
    scheduler: InferenceScheduler,
}

impl Freezable for Inference {}

impl<'cl> CuTask<'cl> for Inference {
    type Input = input_msg!('cl, ImageRgb8Msg);
    type Output = output_msg!('cl, PromptResponseMsg);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        let scheduler = InferenceScheduler::new()
            .map_err(|e| CuError::new_with_cause("Failed to create Paligemma scheduler", e))?;

        Ok(Self {
            current_prompt: DEFAULT_PROMPT.to_string(),
            scheduler,
        })
    }

    fn process(
        &mut self,
        clock: &RobotClock,
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
            .unwrap()
            .try_recv()
        {
            log::debug!("Updating prompt to: {}", prompt);
            self.current_prompt = prompt;
        }

        // check if we are already processing an inference to not block the main thread
        if self.scheduler.is_processing() {
            return Ok(());
        }

        // check first if we have a response from the previous inference
        if let Some(response_msg) = self.scheduler.try_poll_response() {
            log::debug!(
                "Received response from inference thread: {:?}",
                response_msg
            );

            output.metadata.tov = clock.now().into();
            output.set_payload(response_msg);
        }

        // check if we have a new image and schedule the inference
        let Some(img) = input.payload() else {
            return Ok(());
        };

        // send the request to the thread to schedule the inference
        self.scheduler.schedule_inference(img, &self.current_prompt);

        Ok(())
    }
}

struct InferenceScheduler {
    is_processing: Arc<Mutex<AtomicBool>>,
    req_tx: Sender<(ImageRgb8Msg, String)>,
    rep_rx: Receiver<PromptResponseMsg>,
    inference_handle: Option<JoinHandle<Result<(), PaligemmaError>>>,
}

impl InferenceScheduler {
    pub fn new() -> Result<Self, PaligemmaError> {
        // NOTE: in future should be able to schedule multiple models
        let mut paligemma = Paligemma::new(PaligemmaConfig::default())?;

        let (req_tx, req_rx) = std::sync::mpsc::channel::<(ImageRgb8Msg, String)>();
        let (rep_tx, rep_rx) = std::sync::mpsc::channel::<PromptResponseMsg>();

        let is_processing = Arc::new(Mutex::new(AtomicBool::new(false)));

        let inference_handle = std::thread::spawn({
            let is_processing = is_processing.clone();
            move || -> Result<(), PaligemmaError> {
                while let Ok((img, prompt)) = req_rx.recv() {
                    log::trace!("Scheduling a new inference");

                    let response = paligemma.inference(&img, &prompt, 50, false)?;

                    log::trace!("Inference completed");

                    let _ = rep_tx.send(PromptResponseMsg { prompt, response });
                    is_processing
                        .lock()
                        .unwrap()
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                }
                Ok(())
            }
        });

        Ok(Self {
            is_processing,
            req_tx,
            rep_rx,
            inference_handle: Some(inference_handle),
        })
    }

    pub fn is_processing(&self) -> bool {
        self.is_processing
            .lock()
            .unwrap()
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn try_poll_response(&self) -> Option<PromptResponseMsg> {
        self.rep_rx.try_recv().ok()
    }

    pub fn schedule_inference(&self, img: &ImageRgb8Msg, prompt: &str) {
        // TODO: verify that we are not doing a deep copy of the image
        let _ = self.req_tx.send((img.clone(), prompt.to_string()));
        self.is_processing
            .lock()
            .unwrap()
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Drop for InferenceScheduler {
    fn drop(&mut self) {
        if let Some(handle) = self.inference_handle.take() {
            if let Err(_e) = handle.join() {
                log::error!("Failed to join inference thread");
            }
        }
    }
}
