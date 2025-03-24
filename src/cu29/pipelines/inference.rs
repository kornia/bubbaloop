use cu29::prelude::*;
use cu29_helpers::basic_copper_setup;
use std::sync::{atomic::AtomicBool, Arc};

use crate::pipeline::PipelineResult;

const SLAB_SIZE: Option<usize> = Some(150 * 1024 * 1024);

// NOTE: this will use the default config file in the current directory during compilation
// however, it will be overridden by the ron config string when the pipeline is started
#[copper_runtime(config = "src/cu29/pipelines/inference.ron")]
struct InferenceApp {}

pub struct InferencePipeline(pub InferenceApp);

impl InferencePipeline {
    pub fn new() -> CuResult<Self> {
        // NOTE: this is a temporary solution to store the logger in the user's home directory
        let logger_dir = std::path::PathBuf::from(&format!("/home/{}", whoami::username()));
        let logger_path = logger_dir.join("inference.copper");
        debug!("Logger path: {}", path = &logger_path);

        let copper_ctx = basic_copper_setup(&logger_path, SLAB_SIZE, true, None)?;
        let application = InferenceAppBuilder::new()
            .with_context(&copper_ctx)
            .build()?;

        Ok(Self(application))
    }
}

/// Spawns a new thread for the pipeline
///
/// This function is used to spawn a new thread for the pipeline
/// and to pass the stop signal to the pipeline
///
/// # Arguments
///
/// * `pipeline_id` - The id of the pipeline
/// * `stop_signal` - The stop signal to stop the pipeline
///
/// # Returns
///
/// A handle to the thread that runs the pipeline
pub fn spawn_inference_pipeline(
    stop_signal: Arc<AtomicBool>,
) -> std::thread::JoinHandle<PipelineResult> {
    std::thread::spawn({
        let stop_signal = stop_signal.clone();
        move || -> PipelineResult {
            // parse the ron config string and create the pipeline
            let mut app = InferencePipeline::new()?;

            // create the pipeline and start the tasks
            app.start_all_tasks()?;

            while !stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                // we run the pipeline iteration step by step
                app.run_one_iteration()?;

                // NOTE: is this really needed?
                std::thread::sleep(std::time::Duration::from_millis(30));
            }

            // stop the pipeline and wait for the tasks to finish
            app.stop_all_tasks()?;

            log::debug!("Inference pipeline stopped");

            Ok(())
        }
    })
}

impl std::ops::Deref for InferencePipeline {
    type Target = InferenceApp;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for InferencePipeline {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
