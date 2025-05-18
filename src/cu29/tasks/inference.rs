use crate::{
    cu29::msgs::{ImageRgb8Msg, PromptResponseMsg},
    pipeline::SERVER_GLOBAL_STATE,
};
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
const DEFAULT_PROMPT: &str = "cap en\n";

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
        if let Some((channel_id, prompt, response)) = self.scheduler.try_poll_response() {
            log::debug!(
                "Received response from inference thread for channel: {} -- prompt: {} -- response: {}",
                channel_id,
                prompt,
                response
            );

            output.set_payload(PromptResponseMsg {
                stamp_ns: clock.now().as_nanos(),
                channel_id,
                prompt,
                response,
            });
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
    req_tx: Option<Sender<(ImageRgb8Msg, String)>>,
    rep_rx: Receiver<(u8, String, String)>,
    inference_handle: Option<JoinHandle<Result<(), PaligemmaError>>>,
}

impl InferenceScheduler {
    pub fn new() -> Result<Self, PaligemmaError> {
        // NOTE: in future should be able to schedule multiple models
        let mut paligemma = Paligemma::new(PaligemmaConfig::default())?;

        let (req_tx, req_rx) = std::sync::mpsc::channel::<(ImageRgb8Msg, String)>();
        let (rep_tx, rep_rx) = std::sync::mpsc::channel::<(u8, String, String)>();

        let is_processing = Arc::new(Mutex::new(AtomicBool::new(false)));

        let inference_handle = std::thread::spawn({
            let is_processing = is_processing.clone();
            move || -> Result<(), PaligemmaError> {
                // block the thread until the inference is stopped
                while let Ok((img_msg, prompt)) = req_rx.recv() {
                    log::trace!("Scheduling a new inference");

                    let response = paligemma.inference(&img_msg.image, &prompt, 50, false)?;

                    log::trace!("Inference completed");

                    let _ = rep_tx.send((img_msg.channel_id, prompt, response));
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
            req_tx: Some(req_tx),
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

    pub fn try_poll_response(&self) -> Option<(u8, String, String)> {
        self.rep_rx.try_recv().ok()
    }

    pub fn schedule_inference(&self, img: &ImageRgb8Msg, prompt: &str) {
        // SAFETY: we are created the channel in the constructor
        // TODO: verify that we are not doing a deep copy of the image
        let _ = self
            .req_tx
            .as_ref()
            .unwrap()
            .send((img.clone(), prompt.to_string()));
        // set the processing flag to true as we are scheduling an inference
        self.is_processing
            .lock()
            .unwrap()
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn stop(&mut self) {
        // take ownership of the request channel and close it
        drop(self.req_tx.take());

        // join the inference thread
        if let Some(handle) = self.inference_handle.take() {
            if let Err(_e) = handle.join() {
                log::error!("Failed to join inference thread");
            }
        }
    }
}

impl Drop for InferenceScheduler {
    fn drop(&mut self) {
        self.stop();
    }
}
